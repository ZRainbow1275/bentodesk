//! Native shell owner: `rules_execution`.

use super::*;

pub(super) fn run_on_file_change_rules(
    root: &AppRoot,
    event: &bento_nano_backend::watcher::FileChangedEvent,
) -> Result<FileChangeRulesOutcome, RulesWizardError> {
    let state_dir = current_rules_state_dir(root)?;
    let mut outcome = FileChangeRulesOutcome::default();
    if !is_rules_file_change_event(event) {
        return Ok(outcome);
    }

    let rules = rules::load_all(&state_dir);
    let desktop = rules_event_desktop_source(event)?;
    let desktop_string = desktop.to_string_lossy().to_string();

    for rule in rules
        .into_iter()
        .filter(|rule| rule.enabled && matches!(&rule.run_mode, rules::RunMode::OnFileChange))
    {
        outcome.eligible_rules = outcome.eligible_rules.saturating_add(1);
        let preview_zones = {
            let app = root.app.borrow();
            rules_preview_zones_from_app(&app)
        };
        let plan = rule_executor::build_plan(&rule, &desktop_string, &preview_zones, None)
            .map_err(|error| RulesWizardError::Execution(error.to_string()))?;
        let Some(plan) = filter_file_change_execution_plan(&plan, event) else {
            continue;
        };

        let before_snapshot = capture_current_timeline_snapshot(root, "before file-change rule");
        let mut report = apply_rules_execution_plan(root, &plan);
        report.checkpoint_trigger = SmolStr::new_static("rule_file_change_applied");
        record_rule_execution_timeline_pair(root, before_snapshot, &report);
        if report.matched > 0 {
            persist_rule_run_stats(&state_dir, rule.id.as_str())?;
        }

        outcome.triggered_rules = outcome.triggered_rules.saturating_add(1);
        outcome.matched_files = outcome.matched_files.saturating_add(report.matched);
        outcome.action_count = outcome
            .action_count
            .saturating_add(report.actions_taken.len());
        outcome.error_count = outcome.error_count.saturating_add(report.errors.len());
        tracing::info!(
            target: "bentodesk::rules",
            rule_id = %rule.id,
            event_type = %event.event_type,
            path = %event.path,
            matched = report.matched,
            actions = report.actions_taken.len(),
            errors = report.errors.len(),
            "OnFileChange rule applied from desktop watcher event"
        );
    }

    refresh_file_change_rules_status(root, event, outcome)?;
    Ok(outcome)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScheduledRulesOutcome {
    pub(super) eligible_rules: usize,
    pub(super) triggered_rules: usize,
    pub(super) matched_files: usize,
    pub(super) action_count: usize,
    pub(super) error_count: usize,
}

impl ScheduledRulesOutcome {
    pub(super) fn has_visible_status(self) -> bool {
        self.eligible_rules > 0
    }

    pub(super) fn applied_actions(self) -> bool {
        self.triggered_rules > 0 || self.action_count > 0 || self.error_count > 0
    }
}

pub(super) fn interval_rules_status(
    rule_id: &SmolStr,
    outcome: ScheduledRulesOutcome,
    persisted_count: usize,
) -> SmolStr {
    localized_current(
        format!(
            "定时任务：运行 {}/{} 条规则，匹配 {} 个文件，执行 {} 项，错误 {} 项；已保存 {persisted_count} 条规则；规则：{rule_id}",
            outcome.triggered_rules,
            outcome.eligible_rules,
            outcome.matched_files,
            outcome.action_count,
            outcome.error_count
        ),
        format!(
            "Interval scheduler ran {}/{} Interval rules; matched {}; actions={}; errors={}; persisted rules={persisted_count}; rule={rule_id}",
            outcome.triggered_rules,
            outcome.eligible_rules,
            outcome.matched_files,
            outcome.action_count,
            outcome.error_count
        ),
    )
}

pub(super) fn refresh_interval_rules_status(
    root: &AppRoot,
    rule_id: &SmolStr,
    outcome: ScheduledRulesOutcome,
) -> Result<(), RulesWizardError> {
    if !outcome.has_visible_status() {
        return Ok(());
    }
    let count = refresh_rules_wizard(root)?;
    let app = root.app.borrow();
    app.rules_wizard_status
        .borrow_mut()
        .replace(interval_rules_status(rule_id, outcome, count));
    Ok(())
}

pub(super) fn current_unix_secs_for_rules_scheduler() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(super) fn run_interval_rule_for_scheduler_event(
    root: &AppRoot,
    event: &SchedulerEvent,
) -> Result<ScheduledRulesOutcome, RulesWizardError> {
    let desktop = first_desktop_source_for_rules_preview(root)?;
    run_interval_rule_for_scheduler_event_with_desktop(root, event, &desktop)
}

pub(super) fn run_interval_rule_for_scheduler_event_with_desktop(
    root: &AppRoot,
    event: &SchedulerEvent,
    desktop: &Path,
) -> Result<ScheduledRulesOutcome, RulesWizardError> {
    let SchedulerEvent::RuleDue { rule_id } = event;
    let state_dir = current_rules_state_dir(root)?;
    let Some(rule) = rules::load_all(&state_dir)
        .into_iter()
        .find(|rule| rule.id == *rule_id)
    else {
        tracing::debug!(
            target: "bentodesk::rules",
            rule_id = %rule_id,
            "rules scheduler event ignored because persisted rule no longer exists"
        );
        return Ok(ScheduledRulesOutcome::default());
    };

    let mut outcome = ScheduledRulesOutcome::default();
    if !rule.enabled || !matches!(&rule.run_mode, rules::RunMode::Interval { .. }) {
        tracing::debug!(
            target: "bentodesk::rules",
            rule_id = %rule.id,
            "rules scheduler event ignored because rule is disabled or no longer Interval"
        );
        return Ok(outcome);
    }
    if !rule_executor::should_run_now(&rule, current_unix_secs_for_rules_scheduler()) {
        tracing::debug!(
            target: "bentodesk::rules",
            rule_id = %rule.id,
            "rules scheduler event ignored because rule is no longer due"
        );
        return Ok(outcome);
    }

    outcome.eligible_rules = 1;
    let desktop_string = desktop.to_string_lossy().to_string();
    let preview_zones = {
        let app = root.app.borrow();
        rules_preview_zones_from_app(&app)
    };
    let plan = rule_executor::build_plan(&rule, &desktop_string, &preview_zones, None)
        .map_err(|error| RulesWizardError::Execution(error.to_string()))?;
    let before_snapshot = capture_current_timeline_snapshot(root, "before interval rule");
    let mut report = apply_rules_execution_plan(root, &plan);
    report.checkpoint_trigger = SmolStr::new_static("rule_interval_applied");
    record_rule_execution_timeline_pair(root, before_snapshot, &report);
    if report.matched > 0 {
        persist_rule_run_stats(&state_dir, rule.id.as_str())?;
    }

    outcome.triggered_rules = 1;
    outcome.matched_files = report.matched;
    outcome.action_count = report.actions_taken.len();
    outcome.error_count = report.errors.len();
    refresh_interval_rules_status(root, rule_id, outcome)?;
    tracing::info!(
        target: "bentodesk::rules",
        rule_id = %rule.id,
        desktop = %desktop.display(),
        matched = report.matched,
        actions = report.actions_taken.len(),
        errors = report.errors.len(),
        "Interval rule applied from selected-stack rules scheduler event"
    );
    Ok(outcome)
}

pub(super) fn persist_rule_run_stats(
    state_dir: &Path,
    rule_id: &str,
) -> Result<(), RulesWizardError> {
    let mut persisted = rules::load_all(state_dir);
    let Some(rule) = persisted
        .iter_mut()
        .find(|rule| rule.id.as_str() == rule_id)
    else {
        return Err(RulesWizardError::RuleNotFound(SmolStr::new(rule_id)));
    };
    rule.last_run = Some(SmolStr::new(bento_nano_backend::time::now_rfc3339()));
    rule.run_count = rule.run_count.saturating_add(1);
    rules::save_all(state_dir, &persisted)?;
    Ok(())
}

pub(super) fn rules_execution_status(report: &ExecutionReport, persisted_count: usize) -> SmolStr {
    let action_count = report.actions_taken.len();
    let error_count = report.errors.len();
    localized_current(
        format!(
            "运行匹配 {} 个文件，执行 {action_count} 项，错误 {error_count} 项；已保存 {persisted_count} 条规则",
            report.matched
        ),
        format!(
            "Run matched {} files; actions={action_count}; errors={error_count}; persisted rules={persisted_count}",
            report.matched
        ),
    )
}

pub(super) fn apply_rules_execution_plan(root: &AppRoot, plan: &ExecutionPlan) -> ExecutionReport {
    let mut report = ExecutionReport {
        matched: plan.matched.len(),
        actions_taken: Vec::new(),
        errors: Vec::new(),
        checkpoint_trigger: SmolStr::new_static("rule_applied"),
        checkpoint_key: Some(plan.rule_id.clone()),
    };

    for effect in &plan.effects {
        let description = match effect {
            ActionEffect::MoveToZone { zone_id, files } => {
                apply_rules_move_to_zone(root, zone_id, files, &mut report.errors)
            }
            ActionEffect::MoveToFolder { folder, files } => {
                apply_rules_move_to_folder(folder, files, &mut report.errors)
            }
            ActionEffect::DeleteToRecycleBin { files } => {
                apply_rules_delete(files, &mut report.errors)
            }
            ActionEffect::Tag { tags, files } => {
                apply_rules_tag(root, tags, files, &mut report.errors)
            }
            ActionEffect::Notify { message } => format!("Notified: {message}"),
        };
        if !description.is_empty() {
            report.actions_taken.push(description);
        }
    }
    report
}

pub(super) fn rules_zone_id_from_wire(raw: &SmolStr) -> Option<ZoneId> {
    raw.as_str().trim().parse::<u64>().ok().map(ZoneId)
}

pub(super) fn apply_rules_move_to_zone(
    root: &AppRoot,
    raw_zone_id: &SmolStr,
    files: &[bento_nano_backend::grouping::scanner::FileInfo],
    errors: &mut Vec<String>,
) -> String {
    let Some(zone_id) = rules_zone_id_from_wire(raw_zone_id) else {
        errors.push(format!("invalid MoveToZone target: {raw_zone_id}"));
        return "MoveToZone aborted".to_owned();
    };
    if root.app.borrow().zones.get(zone_id).is_none() {
        errors.push(format!("zone not found: {}", zone_id.0));
        return format!("Moved 0 file(s) to zone {}", zone_id.0);
    }
    if let Some(folder) = live_folder_path_for_zone(root, zone_id) {
        errors.push(format!(
            "zone {} is a read-only live folder mirror ({}); use MoveToFolder for filesystem moves",
            zone_id.0,
            folder.display()
        ));
        return format!("Moved 0 file(s) to live-folder zone {}", zone_id.0);
    }

    let mut added = 0usize;
    for file in files {
        if !Path::new(&file.path).exists() {
            errors.push(format!("source file no longer exists: {}", file.path));
            continue;
        }
        let item_path = bento_nano_app::ItemPath::new(file.path.clone());
        let icon_hash = load_icon_hash_for_path(&item_path).unwrap_or_default();
        let hidden = hide_item_file(root, zone_id, &file.path);
        let mut app = root.app.borrow_mut();
        if app
            .zones
            .add_item_with_metadata(
                zone_id,
                std::borrow::Cow::Owned(hidden.effective_path.clone()),
                Some(file.path.as_str()),
                std::borrow::Cow::Owned(icon_hash),
                hidden.original_path.map(std::borrow::Cow::Owned),
                hidden.hidden_path.map(std::borrow::Cow::Owned),
            )
            .is_some()
        {
            app.mark_dirty();
            added += 1;
        } else {
            errors.push(format!("failed to add {} to zone {}", file.path, zone_id.0));
        }
    }
    format!("Moved {added} file(s) to zone {}", zone_id.0)
}

pub(super) fn apply_rules_move_to_folder(
    folder: &str,
    files: &[bento_nano_backend::grouping::scanner::FileInfo],
    errors: &mut Vec<String>,
) -> String {
    let destination = Path::new(folder);
    if let Err(error) = std::fs::create_dir_all(destination) {
        errors.push(format!("failed to create {folder}: {error}"));
        return "MoveToFolder aborted".to_owned();
    }

    let mut moved = 0usize;
    for file in files {
        let source = Path::new(&file.path);
        let Some(file_name) = source.file_name() else {
            errors.push(format!("invalid source path: {}", file.path));
            continue;
        };
        let target = destination.join(file_name);
        match std::fs::rename(source, &target) {
            Ok(()) => moved += 1,
            Err(error) => errors.push(format!(
                "rename {} -> {} failed: {error}",
                file.path,
                target.display()
            )),
        }
    }
    format!("Moved {moved} file(s) to folder {folder}")
}

pub(super) fn tag_target_matches_item(file_path: &str, item: &bento_nano_zone::ZoneItem) -> bool {
    let target = normalized_rules_path(file_path);
    normalized_rules_path(item.path.as_ref()) == target
        || item
            .original_path
            .as_deref()
            .is_some_and(|path| normalized_rules_path(path) == target)
        || item
            .hidden_path
            .as_deref()
            .is_some_and(|path| normalized_rules_path(path) == target)
}

pub(super) fn normalized_rule_tags(tags: &[SmolStr]) -> Vec<String> {
    let mut normalized = Vec::new();
    for tag in tags {
        let value = tag.as_str().trim();
        if value.is_empty() || normalized.iter().any(|existing| existing == value) {
            continue;
        }
        normalized.push(value.to_owned());
    }
    normalized
}

pub(super) fn apply_rules_tag(
    root: &AppRoot,
    tags: &[SmolStr],
    files: &[bento_nano_backend::grouping::scanner::FileInfo],
    errors: &mut Vec<String>,
) -> String {
    let normalized_tags = normalized_rule_tags(tags);
    if normalized_tags.is_empty() {
        errors.push("Tag action has no non-empty tags".to_owned());
        return format!("Tagged 0 item(s) for {} file(s)", files.len());
    }

    let mut tagged_items = 0usize;
    let mut missing_items = 0usize;
    {
        let mut app = root.app.borrow_mut();
        for file in files {
            let mut matched_item = false;
            for zone in app.zones.iter_mut() {
                for item in &mut zone.items {
                    if !tag_target_matches_item(&file.path, item) {
                        continue;
                    }
                    matched_item = true;
                    let mut changed = false;
                    for tag in &normalized_tags {
                        if item.tags.iter().any(|existing| existing.as_ref() == tag) {
                            continue;
                        }
                        item.tags.push(Cow::Owned(tag.clone()));
                        changed = true;
                    }
                    if changed {
                        tagged_items = tagged_items.saturating_add(1);
                    }
                }
            }
            if !matched_item {
                missing_items = missing_items.saturating_add(1);
                errors.push(format!("no zone item found for tag target: {}", file.path));
            }
        }
        if tagged_items > 0 {
            app.mark_dirty();
        }
    }

    format!(
        "Tagged {tagged_items} item(s) for {} file(s) with {:?}; missing={missing_items}",
        files.len(),
        normalized_tags
    )
}

pub(super) fn apply_rules_delete(
    files: &[bento_nano_backend::grouping::scanner::FileInfo],
    errors: &mut Vec<String>,
) -> String {
    let mut recycled = 0usize;
    for file in files {
        let path = Path::new(&file.path);
        if !path.exists() {
            errors.push(format!(
                "recycle delete {} failed: source missing",
                file.path
            ));
            continue;
        }
        match delete_path_to_recycle_bin(path) {
            Ok(RecycleDeleteOutcome::Recycled) => recycled += 1,
            Ok(RecycleDeleteOutcome::Aborted) => {
                errors.push(format!("recycle delete {} aborted by shell", file.path));
            }
            Err(error) => errors.push(format!("recycle delete {} failed: {error}", file.path)),
        }
    }
    format!("Recycled {recycled} file(s)")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecycleDeleteOutcome {
    Recycled,
    Aborted,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum FileOperationError {
    ShellDelete(i32),
}

impl core::fmt::Display for FileOperationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ShellDelete(code) => write!(formatter, "SHFileOperationW failed: {code}"),
        }
    }
}

impl core::error::Error for FileOperationError {}

pub(super) fn shell_file_list_from_path(path: &Path) -> Vec<u16> {
    let mut file_list: Vec<u16> = path.as_os_str().encode_wide().collect();
    file_list.push(0);
    file_list.push(0);
    file_list
}

pub(super) fn recycle_delete_flags() -> u16 {
    (FOF_ALLOWUNDO | FOF_WANTNUKEWARNING | FOF_NOERRORUI | FOF_SILENT) as u16
}

pub(super) fn delete_path_to_recycle_bin(
    path: &Path,
) -> Result<RecycleDeleteOutcome, FileOperationError> {
    // SAFETY: `delete_path_to_recycle_bin_with` builds a valid SHFILEOPSTRUCTW
    // whose double-NUL file list lives for the duration of this synchronous
    // call; the Shell API only reads the struct while `SHFileOperationW` runs.
    delete_path_to_recycle_bin_with(path, |operation| unsafe {
        SHFileOperationW(operation as *mut SHFILEOPSTRUCTW)
    })
}

pub(super) fn delete_path_to_recycle_bin_with<F>(
    path: &Path,
    mut file_operation: F,
) -> Result<RecycleDeleteOutcome, FileOperationError>
where
    F: FnMut(&mut SHFILEOPSTRUCTW) -> i32,
{
    let file_list = shell_file_list_from_path(path);
    let mut operation = SHFILEOPSTRUCTW {
        hwnd: ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: file_list.as_ptr(),
        pTo: ptr::null(),
        fFlags: recycle_delete_flags(),
        fAnyOperationsAborted: 0,
        hNameMappings: ptr::null_mut(),
        lpszProgressTitle: ptr::null(),
    };
    let result = file_operation(&mut operation);
    if result != 0 {
        return Err(FileOperationError::ShellDelete(result));
    }
    if operation.fAnyOperationsAborted != 0 {
        return Ok(RecycleDeleteOutcome::Aborted);
    }
    Ok(RecycleDeleteOutcome::Recycled)
}

pub(super) fn rules_preview_zones_from_app(app: &AppState) -> Vec<BentoZone> {
    bento_zones_from_app(app)
}

pub(super) fn rules_item_type_for_path(path: &str) -> ItemType {
    let path_ref = Path::new(path);
    if path_ref.is_dir() {
        return ItemType::Folder;
    }
    match path_ref
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
    {
        "lnk" | "LNK" | "url" | "URL" => ItemType::Shortcut,
        "exe" | "EXE" | "msi" | "MSI" => ItemType::Application,
        _ => ItemType::File,
    }
}

pub(super) fn rules_percent(value: i32, total: f32) -> f64 {
    if total <= 0.0 {
        return 0.0;
    }
    ((value.max(0) as f32 / total) * 100.0) as f64
}
