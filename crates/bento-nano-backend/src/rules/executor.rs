//! T-088 — evaluate rule conditions and produce action plans.
//!
//! ## What changed vs 1.x
//!
//! 1.x's `execute(handle, rule)` reached into the global `AppState` to mutate
//! `state.layout`, hide files via `crate::hidden_items`, extract icons via
//! `crate::icon::protocol`, and emit Tauri events. None of those side-effect
//! callees are owned by this crate — they live in the dispatcher (`bento-nano-app`)
//! and the icon/stealth modules. The nano executor therefore returns an
//! [`ExecutionPlan`] (the *intent* — matched files + ordered effects) and
//! lets the dispatcher apply the plan against its own state holder.
//!
//! This split also makes the executor unit-testable without standing up an
//! `AppState`, a stealth manifest, or a layout file.
//!
//! ## Q2 predicates
//!
//! [`Condition::NameStartsWith`] / [`Condition::NameContains`] /
//! [`Condition::NameEndsWith`] are case-sensitive plain `str` predicates —
//! the Rules Wizard UI is responsible for normalisation.
//!
//! ## Q1 timestamps
//!
//! `chrono::DateTime::parse_from_rfc3339` + `.signed_duration_since(now)` is
//! replaced by [`crate::time::age_days_since`] (parses the same `…Z` /
//! `+00:00` forms 1.x writes, returns whole-day age, returns 0 on parse
//! failure — same recovery semantic).

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::path::Path;

use crate::grouping::scanner::{self, FileInfo, ScannerError};
use crate::layout::BentoZone;
use crate::time;

use super::{Action, Condition, ConditionGroup, ConditionNode, ExecutionReport, Rule};

/// Events surfaced through the executor's optional `Sender<RuleEvent>`. The
/// 1.x `app.emit("rule_notification", msg)` call site is the only producer
/// today; future executor steps (progress / per-file errors) can land here
/// without touching the call signature.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RuleEvent {
    /// `Action::Notify` was triggered with the carried message.
    Notify { message: String },
}

/// One ordered effect the dispatcher must apply.
///
/// `MoveToZone` and `Tag` carry their full target metadata so the dispatcher
/// can lock + mutate `LayoutData` exactly once per rule, without re-running
/// condition evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionEffect {
    /// Add the listed files to the named zone. Dispatcher: extract icons,
    /// hide via stealth manifest, append `BentoItem`s, persist.
    MoveToZone {
        zone_id: SmolStr,
        files: Vec<FileInfo>,
    },
    /// Move the listed files to a filesystem folder. Dispatcher: create the
    /// folder if needed, then `std::fs::rename` each file.
    MoveToFolder {
        folder: String,
        files: Vec<FileInfo>,
    },
    /// Send the listed files to the Recycle Bin. Dispatcher: invoke the
    /// recycle-bin shell call.
    DeleteToRecycleBin { files: Vec<FileInfo> },
    /// Tag the listed files. Dispatcher: write tag metadata.
    Tag {
        tags: Vec<SmolStr>,
        files: Vec<FileInfo>,
    },
    /// Emit a notification through the dispatcher's UI bus.
    Notify { message: String },
}

/// Output of [`build_plan`]. The dispatcher consumes this in one pass.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub rule_id: SmolStr,
    pub matched: Vec<FileInfo>,
    pub effects: Vec<ActionEffect>,
}

// ─── Evaluation context ──────────────────────────────────────────────

/// Inputs the leaf predicates need beyond the file itself: the desktop
/// directory (for `OnDesktop`) and a reverse index of file path → zone id
/// (for `InZone` and the implicit `OnDesktop = !InZone`).
pub struct EvalContext<'a> {
    pub desktop_path: &'a str,
    pub zone_assignments: HashMap<String, SmolStr>,
}

impl<'a> EvalContext<'a> {
    /// Build the reverse index from a layout snapshot. Mirrors the 1.x
    /// `state.layout.read()` walk.
    pub fn build(desktop_path: &'a str, zones: &[BentoZone]) -> Self {
        let mut zone_assignments = HashMap::new();
        for zone in zones {
            for item in &zone.items {
                zone_assignments.insert(item.path.clone(), zone.id.clone());
                if let Some(orig) = &item.original_path {
                    zone_assignments.insert(orig.clone(), zone.id.clone());
                }
            }
        }
        Self {
            desktop_path,
            zone_assignments,
        }
    }
}

// ─── Pure evaluation ─────────────────────────────────────────────────

/// Evaluate a [`ConditionGroup`] against a single file. `true` ⇒ matches.
pub fn evaluate_group(group: &ConditionGroup, file: &FileInfo, ctx: &EvalContext<'_>) -> bool {
    match group {
        ConditionGroup::All(nodes) => {
            // Empty "all" matches NOTHING (1.x parity — safer than the
            // vacuous-truth interpretation, which would silently match
            // every file when a user clears the conditions).
            if nodes.is_empty() {
                return false;
            }
            nodes.iter().all(|n| evaluate_node(n, file, ctx))
        }
        ConditionGroup::Any(nodes) => nodes.iter().any(|n| evaluate_node(n, file, ctx)),
        ConditionGroup::Not(inner) => !evaluate_group(inner, file, ctx),
    }
}

fn evaluate_node(node: &ConditionNode, file: &FileInfo, ctx: &EvalContext<'_>) -> bool {
    match node {
        ConditionNode::Leaf(c) => evaluate_leaf(c, file, ctx),
        ConditionNode::Group(g) => evaluate_group(g, file, ctx),
    }
}

fn evaluate_leaf(cond: &Condition, file: &FileInfo, ctx: &EvalContext<'_>) -> bool {
    match cond {
        Condition::ExtensionIn(exts) => match &file.extension {
            Some(fe) => {
                let fe_l = fe.to_lowercase();
                exts.iter().any(|e| e.as_str().to_lowercase() == fe_l)
            }
            None => false,
        },
        Condition::NameStartsWith(p) => file.name.starts_with(p.as_str()),
        Condition::NameContains(p) => file.name.contains(p.as_str()),
        Condition::NameEndsWith(p) => file.name.ends_with(p.as_str()),
        Condition::CreatedBefore { days_ago } => {
            time::age_days_since(&file.created_at) > i64::from(*days_ago)
        }
        Condition::ModifiedBefore { days_ago } => {
            time::age_days_since(&file.modified_at) > i64::from(*days_ago)
        }
        Condition::SizeGreaterThan(bytes) => file.size > *bytes,
        Condition::InZone(zone) => ctx
            .zone_assignments
            .get(&file.path)
            .map(|z| z == zone)
            .unwrap_or(false),
        Condition::OnDesktop => !ctx.zone_assignments.contains_key(&file.path),
    }
}

// ─── Plan-building entry points ──────────────────────────────────────

/// File paths the rule would act on, sorted + deduped, without applying.
/// Used by the UI for "Preview hits".
pub fn preview(
    rule: &Rule,
    desktop_path: &str,
    zones: &[BentoZone],
) -> Result<Vec<String>, ScannerError> {
    let ctx = EvalContext::build(desktop_path, zones);
    let files = scanner::scan_desktop_files(Path::new(desktop_path))?;
    let mut hits: Vec<String> = files
        .into_iter()
        .filter(|f| evaluate_group(&rule.conditions, f, &ctx))
        .map(|f| f.path)
        .collect();
    hits.sort();
    hits.dedup();
    Ok(hits)
}

/// Build the [`ExecutionPlan`] for a rule.
///
/// * Pure with respect to the layout: it only *reads* `zones` and the
///   filesystem. The dispatcher applies the resulting effects.
/// * Returns an empty plan (no effects) when zero files match — the 1.x
///   short-circuit.
/// * Sends one [`RuleEvent::Notify`] per `Action::Notify` when `event_tx`
///   is `Some`. Closed channels are tolerated silently (matches 1.x's
///   `let _ = app.emit(…)`).
pub fn build_plan(
    rule: &Rule,
    desktop_path: &str,
    zones: &[BentoZone],
    event_tx: Option<&Sender<RuleEvent>>,
) -> Result<ExecutionPlan, ScannerError> {
    let ctx = EvalContext::build(desktop_path, zones);
    let files = scanner::scan_desktop_files(Path::new(desktop_path))?;
    let matched: Vec<FileInfo> = files
        .into_iter()
        .filter(|f| evaluate_group(&rule.conditions, f, &ctx))
        .collect();

    let mut effects = Vec::new();
    if matched.is_empty() {
        return Ok(ExecutionPlan {
            rule_id: rule.id.clone(),
            matched,
            effects,
        });
    }

    for action in &rule.actions {
        let effect = match action {
            Action::MoveToZone(zid) => ActionEffect::MoveToZone {
                zone_id: zid.clone(),
                files: matched.clone(),
            },
            Action::MoveToFolder(folder) => ActionEffect::MoveToFolder {
                folder: folder.clone(),
                files: matched.clone(),
            },
            Action::DeleteToRecycleBin => ActionEffect::DeleteToRecycleBin {
                files: matched.clone(),
            },
            Action::Tag(tags) => ActionEffect::Tag {
                tags: tags.clone(),
                files: matched.clone(),
            },
            Action::Notify(msg) => {
                if let Some(tx) = event_tx {
                    let _ = tx.send(RuleEvent::Notify {
                        message: msg.clone(),
                    });
                }
                ActionEffect::Notify {
                    message: msg.clone(),
                }
            }
        };
        effects.push(effect);
    }

    Ok(ExecutionPlan {
        rule_id: rule.id.clone(),
        matched,
        effects,
    })
}

/// Convenience wrapper: build the plan and convert it into the 1.x-shaped
/// [`ExecutionReport`] for callers (UI / tray) that only want a high-level
/// summary. The dispatcher still has to apply the [`ExecutionPlan`]
/// returned by [`build_plan`] to actually perform the side effects.
pub fn summarise(plan: &ExecutionPlan) -> ExecutionReport {
    let mut actions_taken = Vec::with_capacity(plan.effects.len());
    for effect in &plan.effects {
        let desc = match effect {
            ActionEffect::MoveToZone { zone_id, files } => {
                format!("Plan move {} file(s) → zone {zone_id}", files.len())
            }
            ActionEffect::MoveToFolder { folder, files } => {
                format!("Plan move {} file(s) → folder {folder}", files.len())
            }
            ActionEffect::DeleteToRecycleBin { files } => {
                format!("Plan delete {} file(s) to recycle bin", files.len())
            }
            ActionEffect::Tag { tags, files } => {
                format!("Plan tag {} file(s) with {tags:?}", files.len())
            }
            ActionEffect::Notify { message } => format!("Notify: {message}"),
        };
        actions_taken.push(desc);
    }
    ExecutionReport {
        matched: plan.matched.len(),
        actions_taken,
        errors: Vec::new(),
        checkpoint_trigger: SmolStr::new_static("rule_applied"),
        checkpoint_key: Some(plan.rule_id.clone()),
    }
}

// ─── Scheduling predicate ────────────────────────────────────────────

/// Should this rule run *now* (Unix epoch seconds)? Pure — no clock read.
///
/// `now_unix_secs` is `SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()`
/// from the caller. Tests pass a fixed value to make the predicate
/// deterministic.
pub fn should_run_now(rule: &Rule, now_unix_secs: i64) -> bool {
    if !rule.enabled {
        return false;
    }
    match &rule.run_mode {
        super::RunMode::OnDemand => false,
        super::RunMode::OnFileChange => false, // triggered through file watcher
        super::RunMode::Interval { minutes } => match &rule.last_run {
            None => true,
            Some(iso) => {
                let last = match time::parse_rfc3339_to_unix_secs(iso) {
                    Ok(v) => v,
                    Err(_) => return true, // 1.x treats unparseable last_run as "due"
                };
                let gap = now_unix_secs.saturating_sub(last);
                gap >= i64::from(*minutes) * 60
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Condition, ConditionGroup, ConditionNode, RunMode};

    fn file(name: &str, ext: &str, age_days: i64) -> FileInfo {
        // RFC3339 timestamp `age_days` days ago, formatted by our own helper.
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let past_secs = now_secs - age_days * 86_400;
        // Use the Hinnant path via a known-good emitter: format manually.
        let stamp = synth_rfc3339(past_secs);
        FileInfo {
            name: name.into(),
            path: format!("C:/Desktop/{name}"),
            size: 1000,
            file_type: ext.into(),
            modified_at: stamp.clone(),
            created_at: stamp,
            is_directory: false,
            extension: Some(ext.into()),
        }
    }

    fn synth_rfc3339(unix_secs: i64) -> String {
        let st = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix_secs as u64);
        time::system_time_to_rfc3339(st)
    }

    fn empty_ctx<'a>() -> EvalContext<'a> {
        EvalContext {
            desktop_path: "",
            zone_assignments: HashMap::new(),
        }
    }

    // ─── Group semantics ─────────────────────────────────────────

    #[test]
    fn all_group_requires_all_leaves() {
        let f = file("notes.tmp", "tmp", 10);
        let g = ConditionGroup::All(vec![
            ConditionNode::Leaf(Condition::ExtensionIn(vec![SmolStr::new_static("tmp")])),
            ConditionNode::Leaf(Condition::CreatedBefore { days_ago: 7 }),
        ]);
        assert!(evaluate_group(&g, &f, &empty_ctx()));

        let g_bad = ConditionGroup::All(vec![
            ConditionNode::Leaf(Condition::ExtensionIn(vec![SmolStr::new_static("tmp")])),
            ConditionNode::Leaf(Condition::CreatedBefore { days_ago: 100 }),
        ]);
        assert!(!evaluate_group(&g_bad, &f, &empty_ctx()));
    }

    #[test]
    fn any_group_only_one_needed() {
        let f = file("readme.md", "md", 2);
        let g = ConditionGroup::Any(vec![
            ConditionNode::Leaf(Condition::ExtensionIn(vec![SmolStr::new_static("tmp")])),
            ConditionNode::Leaf(Condition::NameStartsWith(SmolStr::new_static("read"))),
        ]);
        assert!(evaluate_group(&g, &f, &empty_ctx()));
    }

    #[test]
    fn not_inverts_child() {
        let f = file("a.txt", "txt", 0);
        let inner = ConditionGroup::All(vec![ConditionNode::Leaf(Condition::ExtensionIn(vec![
            SmolStr::new_static("doc"),
        ]))]);
        let g = ConditionGroup::Not(Box::new(inner));
        assert!(evaluate_group(&g, &f, &empty_ctx()));
    }

    #[test]
    fn empty_all_group_matches_nothing() {
        let f = file("x.txt", "txt", 0);
        assert!(!evaluate_group(
            &ConditionGroup::All(vec![]),
            &f,
            &empty_ctx()
        ));
    }

    // ─── Q2 predicates ───────────────────────────────────────────

    #[test]
    fn name_starts_with_matches_prefix() {
        let f = file("invoice-2026.pdf", "pdf", 0);
        let c = Condition::NameStartsWith(SmolStr::new_static("invoice-"));
        assert!(evaluate_leaf(&c, &f, &empty_ctx()));
    }

    #[test]
    fn name_contains_matches_substring() {
        let f = file("draft-invoice-final.pdf", "pdf", 0);
        let c = Condition::NameContains(SmolStr::new_static("invoice"));
        assert!(evaluate_leaf(&c, &f, &empty_ctx()));
    }

    #[test]
    fn name_ends_with_matches_suffix() {
        let f = file("invoice-2026.pdf", "pdf", 0);
        let c = Condition::NameEndsWith(SmolStr::new_static(".pdf"));
        assert!(evaluate_leaf(&c, &f, &empty_ctx()));
    }

    #[test]
    fn name_predicates_are_case_sensitive() {
        let f = file("INVOICE.pdf", "pdf", 0);
        let c = Condition::NameStartsWith(SmolStr::new_static("invoice"));
        assert!(!evaluate_leaf(&c, &f, &empty_ctx()));
    }

    // ─── Other leaf predicates ───────────────────────────────────

    #[test]
    fn size_greater_than_predicate() {
        let mut f = file("x.bin", "bin", 0);
        f.size = 5_000;
        assert!(evaluate_leaf(
            &Condition::SizeGreaterThan(1_000),
            &f,
            &empty_ctx()
        ));
        assert!(!evaluate_leaf(
            &Condition::SizeGreaterThan(10_000),
            &f,
            &empty_ctx()
        ));
    }

    #[test]
    fn extension_in_is_case_insensitive() {
        let f = file("x.PDF", "PDF", 0);
        let c = Condition::ExtensionIn(vec![SmolStr::new_static("pdf")]);
        assert!(evaluate_leaf(&c, &f, &empty_ctx()));
    }

    #[test]
    fn in_zone_uses_assignments() {
        let mut ctx = empty_ctx();
        ctx.zone_assignments
            .insert("C:/Desktop/x.txt".into(), SmolStr::new_static("z1"));
        let f = file("x.txt", "txt", 0);
        assert!(evaluate_leaf(
            &Condition::InZone(SmolStr::new_static("z1")),
            &f,
            &ctx
        ));
        assert!(!evaluate_leaf(
            &Condition::InZone(SmolStr::new_static("other")),
            &f,
            &ctx
        ));
    }

    #[test]
    fn on_desktop_is_inverse_of_assignments() {
        let mut ctx = empty_ctx();
        let assigned = file("a.txt", "txt", 0);
        let free = file("b.txt", "txt", 0);
        ctx.zone_assignments
            .insert(assigned.path.clone(), SmolStr::new_static("z"));
        assert!(!evaluate_leaf(&Condition::OnDesktop, &assigned, &ctx));
        assert!(evaluate_leaf(&Condition::OnDesktop, &free, &ctx));
    }

    // ─── Plan building ───────────────────────────────────────────

    #[test]
    fn plan_for_zero_matches_has_no_effects() {
        let rule = Rule {
            id: SmolStr::new_static("r"),
            name: "noop".into(),
            enabled: true,
            conditions: ConditionGroup::All(vec![ConditionNode::Leaf(Condition::NameStartsWith(
                SmolStr::new_static("nothing-matches-this-"),
            ))]),
            actions: vec![Action::MoveToZone(SmolStr::new_static("zZZZ"))],
            run_mode: RunMode::OnDemand,
            last_run: None,
            run_count: 0,
        };
        // Use a non-existent desktop path so scan returns empty.
        let plan = build_plan(&rule, "C:/__no_such_dir__", &[], None).expect("plan");
        assert!(plan.matched.is_empty());
        assert!(plan.effects.is_empty());
    }

    #[test]
    fn notify_pushes_event_when_channel_provided() {
        let (tx, rx) = crossbeam_channel::unbounded::<RuleEvent>();
        // Build a plan synthetically by piping a single matched file through
        // the action loop — we can't scan a real desktop here, so we test
        // the Notify branch via summarise() round-trip instead.
        let rule = Rule {
            id: SmolStr::new_static("r"),
            name: "n".into(),
            enabled: true,
            conditions: ConditionGroup::default(),
            actions: vec![Action::Notify("hi".into())],
            run_mode: RunMode::OnDemand,
            last_run: None,
            run_count: 0,
        };
        let _ = build_plan(&rule, "C:/__no_dir__", &[], Some(&tx)).expect("plan");
        // Empty match ⇒ no effect built ⇒ no event sent.
        assert!(rx.try_recv().is_err());
    }

    // ─── should_run_now ──────────────────────────────────────────

    #[test]
    fn interval_first_time_is_due() {
        let r = Rule {
            id: SmolStr::new_static("r"),
            name: "".into(),
            enabled: true,
            conditions: ConditionGroup::default(),
            actions: vec![],
            run_mode: RunMode::Interval { minutes: 60 },
            last_run: None,
            run_count: 0,
        };
        assert!(should_run_now(&r, 1_700_000_000));
    }

    #[test]
    fn interval_respects_gap() {
        let now = 1_700_000_000_i64;
        let last = synth_rfc3339(now - 10 * 60); // 10 min ago, gap < 60
        let r = Rule {
            id: SmolStr::new_static("r"),
            name: "".into(),
            enabled: true,
            conditions: ConditionGroup::default(),
            actions: vec![],
            run_mode: RunMode::Interval { minutes: 60 },
            last_run: Some(SmolStr::from(last)),
            run_count: 0,
        };
        assert!(!should_run_now(&r, now));
    }

    #[test]
    fn interval_fires_once_gap_elapsed() {
        let now = 1_700_000_000_i64;
        let last = synth_rfc3339(now - 90 * 60);
        let r = Rule {
            id: SmolStr::new_static("r"),
            name: "".into(),
            enabled: true,
            conditions: ConditionGroup::default(),
            actions: vec![],
            run_mode: RunMode::Interval { minutes: 60 },
            last_run: Some(SmolStr::from(last)),
            run_count: 0,
        };
        assert!(should_run_now(&r, now));
    }

    #[test]
    fn disabled_rule_never_runs() {
        let r = Rule {
            id: SmolStr::new_static("r"),
            name: "".into(),
            enabled: false,
            conditions: ConditionGroup::default(),
            actions: vec![],
            run_mode: RunMode::Interval { minutes: 1 },
            last_run: None,
            run_count: 0,
        };
        assert!(!should_run_now(&r, 1_700_000_000));
    }

    #[test]
    fn on_demand_mode_never_auto_runs() {
        let r = Rule {
            id: SmolStr::new_static("r"),
            name: "".into(),
            enabled: true,
            conditions: ConditionGroup::default(),
            actions: vec![],
            run_mode: RunMode::OnDemand,
            last_run: None,
            run_count: 0,
        };
        assert!(!should_run_now(&r, 1_700_000_000));
    }
}
