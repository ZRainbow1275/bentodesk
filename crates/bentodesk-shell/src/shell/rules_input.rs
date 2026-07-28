//! Native shell owner: `rules_input`.

use super::*;

pub(super) fn handle_rules_wizard_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_ENTER => {
            {
                let app = root.app.borrow();
                let mut wizard = app.rules_wizard.borrow_mut();
                if wizard.step() == WizardStep::Review {
                    wizard.click_save();
                } else {
                    wizard.click_next();
                }
            }
            drain_rules_wizard_action(root, hwnd);
            request_redraw(hwnd);
            0
        }
        VK_BACKSPACE => {
            edit_rules_wizard_text(root, RulesWizardTextEdit::Backspace);
            request_redraw(hwnd);
            0
        }
        VK_F2_KEY => {
            cycle_rules_wizard_condition(root);
            request_redraw(hwnd);
            0
        }
        VK_F3_KEY => {
            cycle_rules_wizard_action(root);
            request_redraw(hwnd);
            0
        }
        VK_F4_KEY => {
            cycle_rules_wizard_run_mode(root);
            request_redraw(hwnd);
            0
        }
        VK_F5_KEY => {
            cycle_rules_wizard_combine(root);
            request_redraw(hwnd);
            0
        }
        VK_SPACE_KEY => {
            let app = root.app.borrow();
            let mut wizard = app.rules_wizard.borrow_mut();
            let next = !wizard.enabled();
            wizard.set_enabled(next);
            app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
                if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                    format!("规则已{}", if next { "启用" } else { "停用" })
                } else {
                    format!("Rule {}", if next { "enabled" } else { "disabled" })
                },
            ));
            request_redraw(hwnd);
            0
        }
        VK_UP_KEY => {
            select_prev_rules_wizard_rule(root);
            request_redraw(hwnd);
            0
        }
        VK_DOWN_KEY => {
            select_next_rules_wizard_rule(root);
            request_redraw(hwnd);
            0
        }
        VK_E_KEY => {
            load_selected_rules_wizard_rule(root);
            request_redraw(hwnd);
            0
        }
        VK_R_KEY => {
            if let Some(rule_id) = selected_rules_wizard_rule_id(root) {
                root.dispatcher.push(Command::RunRuleNow(rule_id));
            } else {
                set_rules_wizard_error(
                    root,
                    SmolStr::new_static(context_menu_text(
                        "尚未选择已保存规则",
                        "No persisted rule selected",
                    )),
                );
            }
            request_redraw(hwnd);
            0
        }
        VK_DELETE_KEY | VK_D_KEY => {
            if let Some(rule_id) = selected_rules_wizard_rule_id(root) {
                if confirm_rules_wizard_delete_or_arm(root, &rule_id) {
                    root.dispatcher.push(Command::DeleteRule(rule_id));
                }
            } else {
                set_rules_wizard_error(
                    root,
                    SmolStr::new_static(context_menu_text(
                        "尚未选择已保存规则",
                        "No persisted rule selected",
                    )),
                );
            }
            request_redraw(hwnd);
            0
        }
        VK_ESCAPE_KEY => {
            {
                let app = root.app.borrow();
                app.rules_wizard.borrow_mut().click_cancel();
            }
            drain_rules_wizard_action(root, hwnd);
            0
        }
        _ => 0,
    }
}

pub(super) fn handle_rules_wizard_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let hit = {
        let app = root.app.borrow();
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
        rules_wizard::rules_wizard_hit_test(
            app.viewport,
            visible_count,
            visible_window_start,
            condition_count,
            condition_window_start,
            x,
            y,
        )
    };
    let Some(hit) = hit else {
        return false;
    };
    let mapped_key = match hit {
        RulesWizardPointerHit::NextSave => Some(VK_ENTER),
        RulesWizardPointerHit::Predicate => Some(VK_F2_KEY),
        RulesWizardPointerHit::Action => Some(VK_F3_KEY),
        RulesWizardPointerHit::RunMode => Some(VK_F4_KEY),
        RulesWizardPointerHit::Combine => Some(VK_F5_KEY),
        RulesWizardPointerHit::AddCondition => {
            add_rules_wizard_condition(root);
            request_redraw(hwnd);
            None
        }
        RulesWizardPointerHit::RemoveCondition => {
            remove_current_rules_wizard_condition(root);
            request_redraw(hwnd);
            None
        }
        RulesWizardPointerHit::NextCondition => {
            select_next_rules_wizard_condition(root);
            request_redraw(hwnd);
            None
        }
        RulesWizardPointerHit::Edit => Some(VK_E_KEY),
        RulesWizardPointerHit::Run => Some(VK_R_KEY),
        RulesWizardPointerHit::Delete => Some(VK_D_KEY),
        RulesWizardPointerHit::Close => Some(VK_ESCAPE_KEY),
        RulesWizardPointerHit::ConditionRow(condition_index) => {
            select_rules_wizard_condition(root, condition_index);
            request_redraw(hwnd);
            None
        }
        RulesWizardPointerHit::Row(row_index) => {
            select_rules_wizard_rule(root, row_index);
            request_redraw(hwnd);
            None
        }
    };
    if let Some(vk) = mapped_key {
        let _ = handle_rules_wizard_keydown(root, vk, hwnd);
    }
    true
}

pub(super) fn handle_rules_wizard_char(root: &AppRoot, wparam: u32) -> bool {
    let Some(ch) = char::from_u32(wparam) else {
        return false;
    };
    if ch.is_control() {
        return false;
    }
    edit_rules_wizard_text(root, RulesWizardTextEdit::Append(ch));
    true
}

pub(super) enum RulesWizardTextEdit {
    Append(char),
    Backspace,
}

pub(super) fn edit_rules_wizard_text(root: &AppRoot, edit: RulesWizardTextEdit) {
    let app = root.app.borrow();
    let mut wizard = app.rules_wizard.borrow_mut();
    match wizard.step() {
        WizardStep::Conditions => {
            let condition_index = wizard.condition_cursor();
            let mut value = wizard
                .conditions()
                .get(condition_index)
                .map(|row| row.value.clone())
                .unwrap_or_default();
            apply_text_edit(&mut value, edit);
            wizard.set_condition_value(condition_index, value);
            wizard.set_error(None);
        }
        WizardStep::Action => {
            let mut value = wizard.action().value.clone();
            apply_text_edit(&mut value, edit);
            wizard.set_action_value(value);
            wizard.set_error(None);
        }
        WizardStep::Name | WizardStep::Review => {
            let mut value = wizard.name().to_owned();
            apply_text_edit(&mut value, edit);
            wizard.set_name(value);
            wizard.set_error(None);
        }
        WizardStep::Preview => {}
    }
}

pub(super) fn apply_text_edit(value: &mut String, edit: RulesWizardTextEdit) {
    match edit {
        RulesWizardTextEdit::Append(ch) => value.push(ch),
        RulesWizardTextEdit::Backspace => {
            let _ = value.pop();
        }
    }
}

pub(super) fn cycle_rules_wizard_condition(root: &AppRoot) {
    let app = root.app.borrow();
    let mut wizard = app.rules_wizard.borrow_mut();
    let condition_index = wizard.condition_cursor();
    let current = wizard
        .conditions()
        .get(condition_index)
        .map(|row| row.kind)
        .unwrap_or_default();
    let next = next_predicate_kind(current);
    wizard.set_condition_kind(condition_index, next);
    wizard.set_error(None);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!(
                "条件 {}：{}",
                condition_index + 1,
                predicate_kind_display_text(next)
            )
        } else {
            format!(
                "Condition {} predicate: {}",
                condition_index + 1,
                predicate_kind_name(next)
            )
        },
    ));
}

pub(super) fn cycle_rules_wizard_action(root: &AppRoot) {
    let app = root.app.borrow();
    let mut wizard = app.rules_wizard.borrow_mut();
    let next = next_action_kind(wizard.action().kind);
    wizard.set_action_kind(next);
    wizard.set_error(None);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("执行操作：{}", action_kind_display_text(next))
        } else {
            format!("Action: {}", action_kind_name(next))
        },
    ));
}

pub(super) fn cycle_rules_wizard_run_mode(root: &AppRoot) {
    let app = root.app.borrow();
    let mut wizard = app.rules_wizard.borrow_mut();
    let next = next_run_mode_choice(wizard.run_mode());
    wizard.set_run_mode(next);
    wizard.set_error(None);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("运行方式：{}", run_mode_choice_display_text(next))
        } else {
            format!("Run mode: {}", run_mode_choice_name(next))
        },
    ));
}

pub(super) fn cycle_rules_wizard_combine(root: &AppRoot) {
    let app = root.app.borrow();
    let mut wizard = app.rules_wizard.borrow_mut();
    let next = match wizard.combine() {
        rules_wizard::CombineMode::All => rules_wizard::CombineMode::Any,
        rules_wizard::CombineMode::Any => rules_wizard::CombineMode::All,
    };
    wizard.set_combine(next);
    wizard.set_error(None);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("条件组合：{}", combine_mode_display_text(next))
        } else {
            format!("Condition combine: {}", combine_mode_name(next))
        },
    ));
}

pub(super) fn add_rules_wizard_condition(root: &AppRoot) {
    let app = root.app.borrow();
    let mut wizard = app.rules_wizard.borrow_mut();
    wizard.add_condition();
    wizard.set_error(None);
    let count = wizard.conditions().len();
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("已添加条件 {count}")
        } else {
            format!("Added condition {count}")
        },
    ));
}

pub(super) fn remove_current_rules_wizard_condition(root: &AppRoot) {
    let app = root.app.borrow();
    let mut wizard = app.rules_wizard.borrow_mut();
    let before = wizard.conditions().len();
    let condition_index = wizard.condition_cursor();
    wizard.remove_condition(condition_index);
    wizard.set_error(None);
    let after = wizard.conditions().len();
    let status = if after == before {
        SmolStr::new_static(context_menu_text(
            "至少需要保留一个条件",
            "At least one condition is required",
        ))
    } else {
        SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!("已移除条件 {}", condition_index + 1)
            } else {
                format!("Removed condition {}", condition_index + 1)
            },
        )
    };
    app.rules_wizard_status.borrow_mut().replace(status);
}

pub(super) fn select_next_rules_wizard_condition(root: &AppRoot) {
    let app = root.app.borrow();
    let mut wizard = app.rules_wizard.borrow_mut();
    wizard.select_next_condition();
    wizard.set_error(None);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!(
                "正在编辑第 {} 个条件，共 {} 个",
                wizard.condition_cursor() + 1,
                wizard.conditions().len()
            )
        } else {
            format!(
                "Editing condition {} of {}",
                wizard.condition_cursor() + 1,
                wizard.conditions().len()
            )
        },
    ));
}

pub(super) fn select_rules_wizard_condition(root: &AppRoot, condition_index: usize) {
    let app = root.app.borrow();
    let mut wizard = app.rules_wizard.borrow_mut();
    wizard.set_condition_cursor(condition_index);
    wizard.set_error(None);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!(
                "正在编辑第 {} 个条件，共 {} 个",
                wizard.condition_cursor() + 1,
                wizard.conditions().len()
            )
        } else {
            format!(
                "Editing condition {} of {}",
                wizard.condition_cursor() + 1,
                wizard.conditions().len()
            )
        },
    ));
}

pub(super) fn next_predicate_kind(current: PredicateKind) -> PredicateKind {
    let all = PredicateKind::ALL;
    let index = all.iter().position(|value| *value == current).unwrap_or(0);
    all[(index + 1) % all.len()]
}

pub(super) fn next_action_kind(current: ActionKind) -> ActionKind {
    let all = ActionKind::ALL;
    let index = all.iter().position(|value| *value == current).unwrap_or(0);
    all[(index + 1) % all.len()]
}

pub(super) fn next_run_mode_choice(current: RunModeChoice) -> RunModeChoice {
    let all = RunModeChoice::ALL;
    let index = all.iter().position(|value| *value == current).unwrap_or(0);
    all[(index + 1) % all.len()]
}

pub(super) fn predicate_kind_name(kind: PredicateKind) -> &'static str {
    match kind {
        PredicateKind::NameStartsWith => "NameStartsWith",
        PredicateKind::NameContains => "NameContains",
        PredicateKind::NameEndsWith => "NameEndsWith",
        PredicateKind::ExtensionIn => "ExtensionIn",
        PredicateKind::CreatedBefore => "CreatedBefore",
        PredicateKind::ModifiedBefore => "ModifiedBefore",
        PredicateKind::SizeGreaterThan => "SizeGreaterThan",
        PredicateKind::InZone => "InZone",
        PredicateKind::OnDesktop => "OnDesktop",
    }
}

pub(super) fn predicate_kind_display_text(kind: PredicateKind) -> &'static str {
    match kind {
        PredicateKind::NameStartsWith => "名称开头是",
        PredicateKind::NameContains => "名称包含",
        PredicateKind::NameEndsWith => "名称结尾是",
        PredicateKind::ExtensionIn => "扩展名属于",
        PredicateKind::CreatedBefore => "创建时间早于指定天数",
        PredicateKind::ModifiedBefore => "修改时间早于指定天数",
        PredicateKind::SizeGreaterThan => "文件大于指定大小",
        PredicateKind::InZone => "位于区域",
        PredicateKind::OnDesktop => "位于桌面",
    }
}

pub(super) fn bulk_sort_key_text(key: bulk_manager_panel::SortKey) -> &'static str {
    match key {
        bulk_manager_panel::SortKey::Name => "名称",
        bulk_manager_panel::SortKey::Items => "项目数",
        bulk_manager_panel::SortKey::Accent => "颜色",
        bulk_manager_panel::SortKey::Size => "尺寸",
    }
}

pub(super) fn bulk_layout_algorithm_text(algorithm: BulkLayoutAlgorithm, zh: bool) -> &'static str {
    match (algorithm, zh) {
        (BulkLayoutAlgorithm::Grid, true) => "网格",
        (BulkLayoutAlgorithm::Row, true) => "横排",
        (BulkLayoutAlgorithm::Column, true) => "纵列",
        (BulkLayoutAlgorithm::Spiral, true) => "环绕",
        (BulkLayoutAlgorithm::Organic, true) => "自然",
        (BulkLayoutAlgorithm::Grid, false) => "grid",
        (BulkLayoutAlgorithm::Row, false) => "row",
        (BulkLayoutAlgorithm::Column, false) => "column",
        (BulkLayoutAlgorithm::Spiral, false) => "spiral",
        (BulkLayoutAlgorithm::Organic, false) => "organic",
    }
}

pub(super) fn bulk_text_field_text(field: BulkTextEditField) -> &'static str {
    match field {
        BulkTextEditField::Alias => "别名",
        BulkTextEditField::Icon => "图标",
        BulkTextEditField::Accent => "颜色",
        BulkTextEditField::CapsuleSize => "胶囊尺寸",
        BulkTextEditField::DisplayMode => "显示模式",
    }
}

pub(super) fn action_kind_name(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::MoveToZone => "MoveToZone",
        ActionKind::MoveToFolder => "MoveToFolder",
        ActionKind::DeleteToRecycleBin => "DeleteToRecycleBin",
        ActionKind::Tag => "Tag",
        ActionKind::Notify => "Notify",
    }
}

pub(super) fn action_kind_display_text(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::MoveToZone => "移动到区域",
        ActionKind::MoveToFolder => "移动到文件夹",
        ActionKind::DeleteToRecycleBin => "移入回收站",
        ActionKind::Tag => "添加标签",
        ActionKind::Notify => "发送通知",
    }
}

pub(super) fn run_mode_choice_name(mode: RunModeChoice) -> &'static str {
    match mode {
        RunModeChoice::OnDemand => "OnDemand",
        RunModeChoice::OnFileChange => "OnFileChange",
        RunModeChoice::Interval => "Interval",
    }
}

pub(super) fn run_mode_choice_display_text(mode: RunModeChoice) -> &'static str {
    match mode {
        RunModeChoice::OnDemand => "手动运行",
        RunModeChoice::OnFileChange => "文件变化时运行",
        RunModeChoice::Interval => "定时运行",
    }
}

pub(super) fn combine_mode_name(mode: rules_wizard::CombineMode) -> &'static str {
    match mode {
        rules_wizard::CombineMode::All => "All",
        rules_wizard::CombineMode::Any => "Any",
    }
}

pub(super) fn combine_mode_display_text(mode: rules_wizard::CombineMode) -> &'static str {
    match mode {
        rules_wizard::CombineMode::All => "全部满足",
        rules_wizard::CombineMode::Any => "任一满足",
    }
}
