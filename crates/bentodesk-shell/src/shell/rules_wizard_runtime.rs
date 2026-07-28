//! Native shell owner: `rules_wizard_runtime`.

use super::*;

#[derive(Debug)]
pub(super) enum RulesWizardError {
    MissingStateDir,
    NoDesktopSource,
    RuleNotFound(SmolStr),
    Platform(PlatformError),
    Persistence(rules::RulesError),
    Preview(String),
    Execution(String),
}

impl core::fmt::Display for RulesWizardError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingStateDir => f.write_str("rules state directory is unavailable"),
            Self::NoDesktopSource => f.write_str("no desktop source is available for preview"),
            Self::RuleNotFound(id) => write!(f, "rule not found: {id}"),
            Self::Platform(source) => write!(f, "platform storage failed: {source}"),
            Self::Persistence(source) => write!(f, "rules persistence failed: {source}"),
            Self::Preview(source) => write!(f, "rules preview failed: {source}"),
            Self::Execution(source) => write!(f, "rules execution failed: {source}"),
        }
    }
}

impl core::error::Error for RulesWizardError {}

impl From<PlatformError> for RulesWizardError {
    fn from(value: PlatformError) -> Self {
        Self::Platform(value)
    }
}

impl From<rules::RulesError> for RulesWizardError {
    fn from(value: rules::RulesError) -> Self {
        Self::Persistence(value)
    }
}

pub(super) fn rules_state_dir_for_zones_path(
    zones_path: &Path,
) -> Result<PathBuf, RulesWizardError> {
    let Some(parent) = zones_path.parent() else {
        return Err(RulesWizardError::MissingStateDir);
    };
    Ok(parent.to_path_buf())
}

pub(super) fn current_rules_state_dir(root: &AppRoot) -> Result<PathBuf, RulesWizardError> {
    let zones_path = root.app.borrow().zones_path.clone();
    if !zones_path.as_os_str().is_empty() {
        return rules_state_dir_for_zones_path(&zones_path);
    }
    let fallback = storage::appdata_path()?;
    rules_state_dir_for_zones_path(&fallback)
}

pub(super) fn new_rule_id() -> SmolStr {
    let stamp = bentodesk_backend::time::now_compact_rfc3339();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.subsec_nanos());
    SmolStr::new(format!("rule-{stamp}-{:x}-{nanos:08x}", std::process::id()))
}

pub(super) fn stamp_rule_id_if_empty(mut rule: Rule) -> Rule {
    if rule.id.as_str().trim().is_empty() {
        rule.id = new_rule_id();
    }
    rule
}

pub(super) fn save_rule_for_state_dir(
    state_dir: &Path,
    rule: Rule,
) -> Result<Rule, RulesWizardError> {
    let rule = stamp_rule_id_if_empty(rule);
    rules::upsert(state_dir, rule.clone())?;
    Ok(rule)
}

pub(super) fn delete_rule_for_state_dir(
    state_dir: &Path,
    rule_id: &str,
) -> Result<(), RulesWizardError> {
    rules::delete(state_dir, rule_id)?;
    Ok(())
}

pub(super) fn refresh_rules_wizard(root: &AppRoot) -> Result<usize, RulesWizardError> {
    let state_dir = current_rules_state_dir(root)?;
    let rules = rules::load_all(&state_dir);
    let count = rules.len();
    let app = root.app.borrow();
    *app.rules_wizard_rules.borrow_mut() = rules;
    app.rules_wizard_delete_confirm.borrow_mut().take();
    if count == 0 {
        app.rules_wizard_rule_cursor.set(0);
    } else {
        let cursor = app.rules_wizard_rule_cursor.get().min(count - 1);
        app.rules_wizard_rule_cursor.set(cursor);
    }
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("已载入 {count} 条规则")
        } else {
            format!("Loaded {count} saved rules")
        },
    ));
    Ok(count)
}

pub(super) fn persist_rule_for_wizard(root: &AppRoot, rule: Rule) -> Result<(), RulesWizardError> {
    let state_dir = current_rules_state_dir(root)?;
    let saved = save_rule_for_state_dir(&state_dir, rule)?;
    let count = refresh_rules_wizard(root)?;
    let app = root.app.borrow();
    app.rules_wizard.borrow_mut().load_rule(saved.clone());
    app.rules_wizard.borrow_mut().set_error(None);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("已保存规则“{}”（共 {count} 条）", saved.name)
        } else {
            format!("Saved rule '{}' ({count} total)", saved.name)
        },
    ));
    Ok(())
}

pub(super) fn delete_rule_for_wizard(
    root: &AppRoot,
    rule_id: &str,
) -> Result<(), RulesWizardError> {
    let state_dir = current_rules_state_dir(root)?;
    delete_rule_for_state_dir(&state_dir, rule_id)?;
    let count = refresh_rules_wizard(root)?;
    let app = root.app.borrow();
    app.rules_wizard.borrow_mut().set_error(None);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("已删除规则（剩余 {count} 条）")
        } else {
            format!("Deleted rule {rule_id} ({count} total)")
        },
    ));
    Ok(())
}

pub(super) fn set_rules_wizard_error(root: &AppRoot, message: SmolStr) {
    let app = root.app.borrow();
    app.rules_wizard.borrow_mut().set_error(Some(message));
    app.rules_wizard_status.borrow_mut().take();
}

pub(super) fn selected_rules_wizard_rule_id(root: &AppRoot) -> Option<SmolStr> {
    let app = root.app.borrow();
    let rules = app.rules_wizard_rules.borrow();
    let cursor = app.rules_wizard_rule_cursor.get();
    rules
        .get(cursor.min(rules.len().saturating_sub(1)))
        .map(|rule| rule.id.clone())
}

pub(super) fn clear_rules_wizard_delete_confirmation(root: &AppRoot) {
    let app = root.app.borrow();
    app.rules_wizard_delete_confirm.borrow_mut().take();
}

pub(super) fn confirm_rules_wizard_delete_or_arm(root: &AppRoot, rule_id: &SmolStr) -> bool {
    let app = root.app.borrow();
    let mut pending = app.rules_wizard_delete_confirm.borrow_mut();
    if pending
        .as_ref()
        .is_some_and(|pending_id| pending_id.as_str() == rule_id.as_str())
    {
        pending.take();
        true
    } else {
        pending.replace(rule_id.clone());
        app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                "再次选择“删除”以确认移除该规则".to_owned()
            } else {
                format!("Press Delete again to permanently remove rule {rule_id}")
            },
        ));
        false
    }
}

pub(super) fn select_rules_wizard_rule(root: &AppRoot, index: usize) {
    let app = root.app.borrow();
    app.rules_wizard_delete_confirm.borrow_mut().take();
    let len = app.rules_wizard_rules.borrow().len();
    if index >= len {
        app.rules_wizard_rule_cursor.set(0);
        app.rules_wizard_status
            .borrow_mut()
            .replace(SmolStr::new_static(context_menu_text(
                "该位置没有可选择的规则",
                "No persisted rule at clicked row",
            )));
        return;
    }
    app.rules_wizard_rule_cursor.set(index);
    app.rules_wizard.borrow_mut().set_error(None);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("已选择第 {} 条规则", index + 1)
        } else {
            format!("Selected rule {}", index + 1)
        },
    ));
}

pub(super) fn select_prev_rules_wizard_rule(root: &AppRoot) {
    let app = root.app.borrow();
    app.rules_wizard_delete_confirm.borrow_mut().take();
    let len = app.rules_wizard_rules.borrow().len();
    if len == 0 {
        app.rules_wizard_rule_cursor.set(0);
        app.rules_wizard_status
            .borrow_mut()
            .replace(SmolStr::new_static(context_menu_text(
                "暂无可选择的已保存规则",
                "No persisted rules to select",
            )));
        return;
    }
    let cursor = app.rules_wizard_rule_cursor.get().saturating_sub(1);
    app.rules_wizard_rule_cursor.set(cursor);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("已选择第 {} 条规则", cursor + 1)
        } else {
            format!("Selected rule {}", cursor + 1)
        },
    ));
}

pub(super) fn select_next_rules_wizard_rule(root: &AppRoot) {
    let app = root.app.borrow();
    app.rules_wizard_delete_confirm.borrow_mut().take();
    let len = app.rules_wizard_rules.borrow().len();
    if len == 0 {
        app.rules_wizard_rule_cursor.set(0);
        app.rules_wizard_status
            .borrow_mut()
            .replace(SmolStr::new_static(context_menu_text(
                "暂无可选择的已保存规则",
                "No persisted rules to select",
            )));
        return;
    }
    let cursor = (app.rules_wizard_rule_cursor.get() + 1).min(len - 1);
    app.rules_wizard_rule_cursor.set(cursor);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("已选择第 {} 条规则", cursor + 1)
        } else {
            format!("Selected rule {}", cursor + 1)
        },
    ));
}

pub(super) fn load_selected_rules_wizard_rule(root: &AppRoot) {
    let app = root.app.borrow();
    app.rules_wizard_delete_confirm.borrow_mut().take();
    let rules = app.rules_wizard_rules.borrow();
    let cursor = app.rules_wizard_rule_cursor.get();
    let Some(rule) = rules
        .get(cursor.min(rules.len().saturating_sub(1)))
        .cloned()
    else {
        drop(rules);
        app.rules_wizard
            .borrow_mut()
            .set_error(Some(SmolStr::new_static(context_menu_text(
                "尚未选择已保存规则",
                "No persisted rule selected",
            ))));
        app.rules_wizard_status.borrow_mut().take();
        return;
    };
    let name = rule.name.clone();
    drop(rules);
    app.rules_wizard.borrow_mut().load_rule(rule);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("正在编辑规则“{name}”")
        } else {
            format!("Editing rule '{name}'")
        },
    ));
}

pub(super) fn drain_rules_wizard_action(root: &AppRoot, hwnd: HWND) {
    let action = {
        let app = root.app.borrow();
        app.rules_wizard.borrow_mut().take_action()
    };
    match action {
        Some(RulesWizardAction::Save(rule)) => {
            root.dispatcher.push(Command::SaveRule(rule));
        }
        Some(RulesWizardAction::PreviewRequest(rule)) => {
            root.dispatcher.push(Command::PreviewRuleHits(rule));
        }
        Some(RulesWizardAction::Cancel) => unsafe {
            ShowWindow(hwnd, SW_HIDE);
        },
        None => {}
    }
}

pub(super) fn first_desktop_source_for_rules_preview(
    root: &AppRoot,
) -> Result<PathBuf, RulesWizardError> {
    configured_desktop_sources_for_app(&root.app.borrow())
        .into_iter()
        .next()
        .ok_or(RulesWizardError::NoDesktopSource)
}

pub(super) fn preview_rule_for_wizard(
    root: &AppRoot,
    rule: &Rule,
) -> Result<usize, RulesWizardError> {
    let desktop = first_desktop_source_for_rules_preview(root)?;
    let desktop_string = desktop.to_string_lossy().to_string();
    let preview_zones = {
        let app = root.app.borrow();
        rules_preview_zones_from_app(&app)
    };
    let hits = rule_executor::preview(rule, &desktop_string, &preview_zones)
        .map_err(|error| RulesWizardError::Preview(error.to_string()))?;
    let count = hits.len();
    let app = root.app.borrow();
    let mut wizard = app.rules_wizard.borrow_mut();
    wizard.set_preview_hits(hits);
    wizard.set_error(None);
    drop(wizard);
    app.rules_wizard_status.borrow_mut().replace(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("预览命中 {count} 个桌面文件")
        } else {
            format!("Preview matched {count} desktop files")
        },
    ));
    Ok(count)
}

pub(super) fn run_rule_now_for_wizard(
    root: &AppRoot,
    rule_id: &SmolStr,
) -> Result<ExecutionReport, RulesWizardError> {
    let state_dir = current_rules_state_dir(root)?;
    let rule = rules::load_all(&state_dir)
        .into_iter()
        .find(|rule| rule.id == *rule_id)
        .ok_or_else(|| RulesWizardError::RuleNotFound(rule_id.clone()))?;
    let desktop = first_desktop_source_for_rules_preview(root)?;
    let desktop_string = desktop.to_string_lossy().to_string();
    let preview_zones = {
        let app = root.app.borrow();
        rules_preview_zones_from_app(&app)
    };
    let plan = rule_executor::build_plan(&rule, &desktop_string, &preview_zones, None)
        .map_err(|error| RulesWizardError::Execution(error.to_string()))?;
    let before_snapshot = capture_current_timeline_snapshot(root, "before rule run");
    let report = apply_rules_execution_plan(root, &plan);
    record_rule_execution_timeline_pair(root, before_snapshot, &report);
    if report.matched > 0 {
        persist_rule_run_stats(&state_dir, rule.id.as_str())?;
    }
    let count = refresh_rules_wizard(root)?;
    let updated_rule = {
        let app = root.app.borrow();
        app.rules_wizard_rules
            .borrow()
            .iter()
            .find(|candidate| candidate.id == rule.id)
            .cloned()
    };
    let app = root.app.borrow();
    if let Some(updated_rule) = updated_rule {
        app.rules_wizard.borrow_mut().load_rule(updated_rule);
    }
    app.rules_wizard.borrow_mut().set_error(None);
    app.rules_wizard_status
        .borrow_mut()
        .replace(rules_execution_status(&report, count));
    Ok(report)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileChangeRulesOutcome {
    pub(super) eligible_rules: usize,
    pub(super) triggered_rules: usize,
    pub(super) matched_files: usize,
    pub(super) action_count: usize,
    pub(super) error_count: usize,
}

impl FileChangeRulesOutcome {
    pub(super) fn has_visible_status(self) -> bool {
        self.eligible_rules > 0
    }

    pub(super) fn applied_actions(self) -> bool {
        self.triggered_rules > 0 || self.action_count > 0 || self.error_count > 0
    }
}

pub(super) fn is_rules_file_change_event(
    event: &bentodesk_backend::watcher::FileChangedEvent,
) -> bool {
    matches!(event.event_type.as_str(), "create" | "modify")
}

pub(super) fn rules_event_desktop_source(
    event: &bentodesk_backend::watcher::FileChangedEvent,
) -> Result<PathBuf, RulesWizardError> {
    Path::new(&event.path)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .ok_or(RulesWizardError::NoDesktopSource)
}

pub(super) fn normalized_rules_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    normalized.to_ascii_lowercase()
}

pub(super) fn file_change_path_matches(
    event: &bentodesk_backend::watcher::FileChangedEvent,
    candidate_path: &str,
) -> bool {
    normalized_rules_path(candidate_path) == normalized_rules_path(&event.path)
}

pub(super) fn filter_file_change_files(
    event: &bentodesk_backend::watcher::FileChangedEvent,
    files: &[bentodesk_backend::grouping::scanner::FileInfo],
) -> Vec<bentodesk_backend::grouping::scanner::FileInfo> {
    files
        .iter()
        .filter(|file| file_change_path_matches(event, &file.path))
        .cloned()
        .collect()
}

pub(super) fn filter_file_change_execution_plan(
    plan: &ExecutionPlan,
    event: &bentodesk_backend::watcher::FileChangedEvent,
) -> Option<ExecutionPlan> {
    let matched = filter_file_change_files(event, &plan.matched);
    if matched.is_empty() {
        return None;
    }

    let mut effects = Vec::new();
    for effect in &plan.effects {
        match effect {
            ActionEffect::MoveToZone { zone_id, files } => {
                let files = filter_file_change_files(event, files);
                if !files.is_empty() {
                    effects.push(ActionEffect::MoveToZone {
                        zone_id: zone_id.clone(),
                        files,
                    });
                }
            }
            ActionEffect::MoveToFolder { folder, files } => {
                let files = filter_file_change_files(event, files);
                if !files.is_empty() {
                    effects.push(ActionEffect::MoveToFolder {
                        folder: folder.clone(),
                        files,
                    });
                }
            }
            ActionEffect::DeleteToRecycleBin { files } => {
                let files = filter_file_change_files(event, files);
                if !files.is_empty() {
                    effects.push(ActionEffect::DeleteToRecycleBin { files });
                }
            }
            ActionEffect::Tag { tags, files } => {
                let files = filter_file_change_files(event, files);
                if !files.is_empty() {
                    effects.push(ActionEffect::Tag {
                        tags: tags.clone(),
                        files,
                    });
                }
            }
            ActionEffect::Notify { message } => {
                effects.push(ActionEffect::Notify {
                    message: message.clone(),
                });
            }
        }
    }

    Some(ExecutionPlan {
        rule_id: plan.rule_id.clone(),
        matched,
        effects,
    })
}

pub(super) fn file_change_rules_status(
    event: &bentodesk_backend::watcher::FileChangedEvent,
    outcome: FileChangeRulesOutcome,
    persisted_count: usize,
) -> SmolStr {
    localized_current(
        format!(
            "文件变更 {}：运行 {}/{} 条规则，匹配 {} 个文件，执行 {} 项，错误 {} 项；已保存 {persisted_count} 条规则；路径：{}",
            event.event_type,
            outcome.triggered_rules,
            outcome.eligible_rules,
            outcome.matched_files,
            outcome.action_count,
            outcome.error_count,
            event.path
        ),
        format!(
            "File change {} ran {}/{} OnFileChange rules; matched {}; actions={}; errors={}; persisted rules={persisted_count}; path={}",
            event.event_type,
            outcome.triggered_rules,
            outcome.eligible_rules,
            outcome.matched_files,
            outcome.action_count,
            outcome.error_count,
            event.path
        ),
    )
}

pub(super) fn refresh_file_change_rules_status(
    root: &AppRoot,
    event: &bentodesk_backend::watcher::FileChangedEvent,
    outcome: FileChangeRulesOutcome,
) -> Result<(), RulesWizardError> {
    if !outcome.has_visible_status() {
        return Ok(());
    }
    let count = refresh_rules_wizard(root)?;
    let app = root.app.borrow();
    app.rules_wizard_status
        .borrow_mut()
        .replace(file_change_rules_status(event, outcome, count));
    Ok(())
}
