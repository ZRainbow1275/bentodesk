//! Evaluate rule conditions and execute action chains.

use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::BentoDeskError;
use crate::grouping::scanner::{self, FileInfo};
use crate::layout::persistence::{BentoItem, GridPosition, ItemType};
use crate::timeline::hook as timeline_hook;
use crate::AppState;

use super::{Action, Condition, ConditionGroup, ConditionNode, ExecutionReport, Rule};

/// Context passed around during condition evaluation so path-based predicates
/// (InZone / OnDesktop) can consult the current layout.
struct EvalContext {
    desktop_path: String,
    zone_assignments: std::collections::HashMap<String, String>, // path → zone_id
}

fn build_context(app: &AppHandle) -> EvalContext {
    let state = app.state::<AppState>();
    let desktop_path = state
        .settings
        .lock()
        .map(|s| s.desktop_path.clone())
        .unwrap_or_default();
    let mut map = std::collections::HashMap::new();
    if let Ok(layout) = state.layout.lock() {
        for zone in &layout.zones {
            for item in &zone.items {
                map.insert(item.path.clone(), zone.id.clone());
                if let Some(orig) = &item.original_path {
                    map.insert(orig.clone(), zone.id.clone());
                }
            }
        }
    }
    EvalContext {
        desktop_path,
        zone_assignments: map,
    }
}

/// Evaluate a `ConditionGroup` against a file. Returns `true` when the file matches.
fn evaluate_group(group: &ConditionGroup, file: &FileInfo, ctx: &EvalContext) -> bool {
    match group {
        ConditionGroup::All(nodes) => {
            if nodes.is_empty() {
                // Empty "all" matches nothing — safer than matching everything.
                return false;
            }
            nodes.iter().all(|n| evaluate_node(n, file, ctx))
        }
        ConditionGroup::Any(nodes) => nodes.iter().any(|n| evaluate_node(n, file, ctx)),
        ConditionGroup::Not(inner) => !evaluate_group(inner, file, ctx),
    }
}

fn evaluate_node(node: &ConditionNode, file: &FileInfo, ctx: &EvalContext) -> bool {
    match node {
        ConditionNode::Leaf(cond) => evaluate_leaf(cond, file, ctx),
        ConditionNode::Group(g) => evaluate_group(g, file, ctx),
    }
}

fn evaluate_leaf(cond: &Condition, file: &FileInfo, ctx: &EvalContext) -> bool {
    match cond {
        Condition::ExtensionIn(exts) => match &file.extension {
            Some(fe) => {
                let fe_l = fe.to_lowercase();
                exts.iter().any(|e| e.to_lowercase() == fe_l)
            }
            None => false,
        },
        Condition::NameMatchesRegex(pat) => match Regex::new(pat) {
            Ok(re) => re.is_match(&file.name),
            Err(_) => false,
        },
        Condition::CreatedBefore { days_ago } => age_days(&file.created_at) > *days_ago as i64,
        Condition::ModifiedBefore { days_ago } => age_days(&file.modified_at) > *days_ago as i64,
        Condition::SizeGreaterThan(bytes) => file.size > *bytes,
        Condition::InZone(zone) => ctx
            .zone_assignments
            .get(&file.path)
            .map(|z| z == zone)
            .unwrap_or(false),
        Condition::OnDesktop => !ctx.zone_assignments.contains_key(&file.path),
    }
}

fn age_days(iso: &str) -> i64 {
    if iso.is_empty() {
        return 0;
    }
    match DateTime::parse_from_rfc3339(iso) {
        Ok(t) => Utc::now().signed_duration_since(t).num_days(),
        Err(_) => 0,
    }
}

/// Return the list of file paths the rule would act on, without applying.
/// Used by the UI for "Preview hits".
pub fn preview(app: &AppHandle, rule: &Rule) -> Result<Vec<String>, BentoDeskError> {
    let ctx = build_context(app);
    let desktop = Path::new(&ctx.desktop_path);
    let files = scanner::scan_desktop_files(desktop).unwrap_or_default();
    let mut hits: Vec<String> = files
        .into_iter()
        .filter(|f| evaluate_group(&rule.conditions, f, &ctx))
        .map(|f| f.path)
        .collect();
    hits.sort();
    hits.dedup();
    Ok(hits)
}

/// Execute a rule now. Hooks the timeline so Ctrl+Z can undo the batch.
pub fn execute(app: &AppHandle, rule: &Rule) -> Result<ExecutionReport, BentoDeskError> {
    let ctx = build_context(app);
    let desktop = Path::new(&ctx.desktop_path);
    let files = scanner::scan_desktop_files(desktop).unwrap_or_default();
    let matched: Vec<FileInfo> = files
        .into_iter()
        .filter(|f| evaluate_group(&rule.conditions, f, &ctx))
        .collect();

    let mut report = ExecutionReport {
        matched: matched.len(),
        actions_taken: Vec::new(),
        errors: Vec::new(),
        checkpoint_trigger: "rule_applied".to_string(),
    };

    if matched.is_empty() {
        return Ok(report);
    }

    for action in &rule.actions {
        let desc = match action {
            Action::MoveToZone(zid) => apply_move_to_zone(app, zid, &matched, &mut report.errors),
            Action::MoveToFolder(folder) => {
                apply_move_to_folder(folder, &matched, &mut report.errors)
            }
            Action::DeleteToRecycleBin => apply_delete(&matched, &mut report.errors),
            Action::Tag(tags) => format!("Tagged {} files with {:?}", matched.len(), tags),
            Action::Notify(msg) => {
                let _ = app.emit("rule_notification", msg);
                format!("Notified: {msg}")
            }
        };
        report.actions_taken.push(desc);
    }

    // Post-hook the timeline so the checkpoint captures the post-mutation
    // state; Ctrl+Z then diffs against the prior snapshot to recover the
    // pre-rule state. Invoking this before mutations made the delta empty.
    timeline_hook::record_change(app, "rule_applied");

    // Record post-execution timestamp + count.
    update_rule_stats(app, &rule.id);

    Ok(report)
}

fn apply_move_to_zone(
    app: &AppHandle,
    zone_id: &str,
    matched: &[FileInfo],
    errors: &mut Vec<String>,
) -> String {
    let state = app.state::<AppState>();
    let mut added = 0usize;

    // Hide files + register layout items under a single lock window.
    let mut prepared = Vec::new();
    for file in matched {
        match crate::icon::protocol::extract_and_cache_fresh(&state.icon_cache, &file.path) {
            Ok(hash) => {
                let (orig, hidden) =
                    match crate::hidden_items::hide_file(&state.app_handle, &file.path, zone_id) {
                        Some((o, h)) => (Some(o), Some(h)),
                        None => (None, None),
                    };
                let effective = hidden.clone().unwrap_or_else(|| file.path.clone());
                prepared.push((file.clone(), hash, orig, hidden, effective));
            }
            Err(e) => errors.push(format!("icon extract failed for {}: {e}", file.path)),
        }
    }

    {
        let mut layout = match state.layout.lock() {
            Ok(l) => l,
            Err(e) => {
                errors.push(format!("layout lock poisoned: {e}"));
                return String::new();
            }
        };
        let zone = match layout.zones.iter_mut().find(|z| z.id == zone_id) {
            Some(z) => z,
            None => {
                errors.push(format!("Zone not found: {zone_id}"));
                return String::new();
            }
        };
        for (file, hash, orig, hidden, effective) in prepared {
            let idx = zone.items.len() as u32;
            let ext = Path::new(&file.path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let item = BentoItem {
                id: uuid::Uuid::new_v4().to_string(),
                zone_id: zone_id.to_string(),
                item_type: match ext {
                    "lnk" => ItemType::Shortcut,
                    "exe" | "msi" => ItemType::Application,
                    _ => ItemType::File,
                },
                name: file.name.clone(),
                path: effective,
                icon_hash: hash,
                grid_position: GridPosition {
                    col: idx % zone.grid_columns,
                    row: idx / zone.grid_columns,
                    col_span: 1,
                },
                is_wide: false,
                added_at: chrono::Utc::now().to_rfc3339(),
                original_path: orig,
                hidden_path: hidden,
                icon_x: None,
                icon_y: None,
                file_missing: false,
            };
            zone.items.push(item);
            added += 1;
        }
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();

    format!("Moved {added} file(s) to zone {zone_id}")
}

fn apply_move_to_folder(folder: &str, matched: &[FileInfo], errors: &mut Vec<String>) -> String {
    let dest = Path::new(folder);
    if !dest.exists() {
        if let Err(e) = std::fs::create_dir_all(dest) {
            errors.push(format!("failed to create {folder}: {e}"));
            return "MoveToFolder aborted".into();
        }
    }
    let mut ok = 0usize;
    for f in matched {
        let src = Path::new(&f.path);
        let name = match src.file_name() {
            Some(n) => n,
            None => {
                errors.push(format!("invalid path: {}", f.path));
                continue;
            }
        };
        let dst = dest.join(name);
        match std::fs::rename(src, &dst) {
            Ok(()) => ok += 1,
            Err(e) => errors.push(format!("rename {} → {}: {e}", f.path, dst.display())),
        }
    }
    format!("Moved {ok} file(s) to folder {folder}")
}

fn apply_delete(matched: &[FileInfo], errors: &mut Vec<String>) -> String {
    let mut ok = 0usize;
    for f in matched {
        match std::fs::remove_file(&f.path) {
            Ok(()) => ok += 1,
            Err(e) => errors.push(format!("delete {}: {e}", f.path)),
        }
    }
    format!("Deleted {ok} file(s)")
}

fn update_rule_stats(app: &AppHandle, rule_id: &str) {
    let mut rules = super::load_all(app);
    if let Some(rule) = rules.iter_mut().find(|r| r.id == rule_id) {
        rule.last_run = Some(Utc::now().to_rfc3339());
        rule.run_count += 1;
    }
    if let Err(e) = super::save_all(app, &rules) {
        tracing::warn!("Failed to persist rule stats: {e}");
    }
}

/// Does the rule want to run at this point in time?
pub fn should_run_now(rule: &Rule, now: DateTime<Utc>) -> bool {
    if !rule.enabled {
        return false;
    }
    match &rule.run_mode {
        super::RunMode::OnDemand => false,
        super::RunMode::OnFileChange => false, // triggered via file watcher path
        super::RunMode::Interval { minutes } => match &rule.last_run {
            None => true,
            Some(iso) => match DateTime::parse_from_rfc3339(iso) {
                Ok(t) => now.signed_duration_since(t) >= Duration::minutes(*minutes as i64),
                Err(_) => true,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Condition, ConditionGroup, ConditionNode, RunMode};

    fn file(name: &str, ext: &str, age_days: i64) -> FileInfo {
        let created = (Utc::now() - Duration::days(age_days)).to_rfc3339();
        FileInfo {
            name: name.into(),
            path: format!("C:/Desktop/{name}"),
            size: 1000,
            file_type: ext.into(),
            modified_at: created.clone(),
            created_at: created,
            is_directory: false,
            extension: Some(ext.into()),
        }
    }

    fn empty_ctx() -> EvalContext {
        EvalContext {
            desktop_path: String::new(),
            zone_assignments: Default::default(),
        }
    }

    #[test]
    fn all_group_requires_all_leaves() {
        let f = file("notes.tmp", "tmp", 10);
        let g = ConditionGroup::All(vec![
            ConditionNode::Leaf(Condition::ExtensionIn(vec!["tmp".into()])),
            ConditionNode::Leaf(Condition::CreatedBefore { days_ago: 7 }),
        ]);
        assert!(evaluate_group(&g, &f, &empty_ctx()));

        let g_bad = ConditionGroup::All(vec![
            ConditionNode::Leaf(Condition::ExtensionIn(vec!["tmp".into()])),
            ConditionNode::Leaf(Condition::CreatedBefore { days_ago: 100 }),
        ]);
        assert!(!evaluate_group(&g_bad, &f, &empty_ctx()));
    }

    #[test]
    fn any_group_only_one_needed() {
        let f = file("readme.md", "md", 2);
        let g = ConditionGroup::Any(vec![
            ConditionNode::Leaf(Condition::ExtensionIn(vec!["tmp".into()])),
            ConditionNode::Leaf(Condition::NameMatchesRegex("readme".into())),
        ]);
        assert!(evaluate_group(&g, &f, &empty_ctx()));
    }

    #[test]
    fn not_inverts_child() {
        let f = file("a.txt", "txt", 0);
        let inner = ConditionGroup::All(vec![ConditionNode::Leaf(Condition::ExtensionIn(vec![
            "doc".into(),
        ]))]);
        let g = ConditionGroup::Not(Box::new(inner));
        assert!(evaluate_group(&g, &f, &empty_ctx()));
    }

    #[test]
    fn interval_run_mode_first_time() {
        let r = Rule {
            id: "r".into(),
            name: "".into(),
            enabled: true,
            conditions: ConditionGroup::default(),
            actions: vec![],
            run_mode: RunMode::Interval { minutes: 60 },
            last_run: None,
            run_count: 0,
        };
        assert!(should_run_now(&r, Utc::now()));
    }

    #[test]
    fn interval_run_mode_respects_gap() {
        let r = Rule {
            id: "r".into(),
            name: "".into(),
            enabled: true,
            conditions: ConditionGroup::default(),
            actions: vec![],
            run_mode: RunMode::Interval { minutes: 60 },
            last_run: Some(Utc::now().to_rfc3339()),
            run_count: 0,
        };
        assert!(!should_run_now(&r, Utc::now()));
    }
}
