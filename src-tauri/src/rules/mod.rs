//! Outlook-style Rules engine — data model + persistence.
//!
//! Persisted to `%APPDATA%/BentoDesk/rules.json` with an atomic write +
//! `.bak` sibling so a crash between rename steps can never lose the rules.
//!
//! **Time-machine integration**: every execution emits a `record_change`
//! hook labelled `rule_applied` so the user can `Ctrl+Z` a bulk move.

pub mod executor;
pub mod scheduler;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::BentoDeskError;

/// A Rule bundles a condition tree with an ordered action list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub conditions: ConditionGroup,
    pub actions: Vec<Action>,
    #[serde(default)]
    pub run_mode: RunMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<String>,
    #[serde(default)]
    pub run_count: u64,
}

fn default_enabled() -> bool {
    true
}

/// Boolean tree of conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ConditionGroup {
    /// All child conditions must be true (logical AND).
    All(Vec<ConditionNode>),
    /// Any child condition must be true (logical OR).
    Any(Vec<ConditionNode>),
    /// Negated subtree.
    Not(Box<ConditionGroup>),
}

impl Default for ConditionGroup {
    fn default() -> Self {
        ConditionGroup::All(Vec::new())
    }
}

/// Either a leaf `Condition` or a nested group.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConditionNode {
    Leaf(Condition),
    Group(ConditionGroup),
}

/// Individual file predicate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Condition {
    /// Extension is in the provided set (case-insensitive).
    ExtensionIn(Vec<String>),
    /// Filename matches the given regex.
    NameMatchesRegex(String),
    /// File was created strictly before `now - days_ago`.
    CreatedBefore { days_ago: u32 },
    /// File was last modified strictly before `now - days_ago`.
    ModifiedBefore { days_ago: u32 },
    /// Size is strictly greater than this many bytes.
    SizeGreaterThan(u64),
    /// File is currently assigned to the given zone id.
    InZone(String),
    /// File lives directly on the desktop (not in a zone).
    OnDesktop,
}

/// Action to execute on a matched file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Action {
    /// Move / add the file into a specific zone.
    MoveToZone(String),
    /// Move the raw file to a filesystem folder.
    MoveToFolder(String),
    /// Send to the Recycle Bin.
    DeleteToRecycleBin,
    /// Attach tag(s) to the file (in-layout metadata).
    Tag(Vec<String>),
    /// Emit a toast notification with a message.
    Notify(String),
}

/// When should the rule run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum RunMode {
    /// Only on manual "Run now".
    #[default]
    OnDemand,
    /// After every file system change on the desktop.
    OnFileChange,
    /// Periodically, every N minutes.
    Interval { minutes: u32 },
}

/// Summary of a single execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionReport {
    pub matched: usize,
    pub actions_taken: Vec<String>,
    pub errors: Vec<String>,
    pub checkpoint_trigger: String,
}

static PERSIST_LOCK: Mutex<()> = Mutex::new(());

fn rules_dir(handle: &AppHandle) -> PathBuf {
    handle
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn rules_path(handle: &AppHandle) -> PathBuf {
    rules_dir(handle).join("rules.json")
}

fn rules_backup_path(handle: &AppHandle) -> PathBuf {
    rules_dir(handle).join("rules.json.bak")
}

/// Load all persisted rules. Returns an empty list if the file is missing.
pub fn load_all(handle: &AppHandle) -> Vec<Rule> {
    let path = rules_path(handle);
    read_rules(&path)
}

fn read_rules(path: &Path) -> Vec<Rule> {
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Persist the rule list atomically. On failure the previous `rules.json`
/// is preserved at `.bak` so callers can still recover.
pub fn save_all(handle: &AppHandle, rules: &[Rule]) -> Result<(), BentoDeskError> {
    let _g = PERSIST_LOCK
        .lock()
        .map_err(|e| BentoDeskError::Generic(format!("rules save lock: {e}")))?;

    let dir = rules_dir(handle);
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }

    let path = rules_path(handle);
    let tmp = path.with_extension("json.tmp");
    let backup = rules_backup_path(handle);

    let json = serde_json::to_string_pretty(rules)?;
    fs::write(&tmp, json)?;

    if path.exists() {
        let _ = fs::copy(&path, &backup);
    }
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Insert or replace by id.
pub fn upsert(handle: &AppHandle, rule: Rule) -> Result<(), BentoDeskError> {
    let mut rules = load_all(handle);
    if let Some(existing) = rules.iter_mut().find(|r| r.id == rule.id) {
        *existing = rule;
    } else {
        rules.push(rule);
    }
    save_all(handle, &rules)
}

/// Remove a rule by id.
pub fn delete(handle: &AppHandle, id: &str) -> Result<(), BentoDeskError> {
    let mut rules = load_all(handle);
    rules.retain(|r| r.id != id);
    save_all(handle, &rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_group_defaults_to_all() {
        let grp = ConditionGroup::default();
        match grp {
            ConditionGroup::All(v) => assert!(v.is_empty()),
            _ => panic!("expected All"),
        }
    }

    #[test]
    fn rule_roundtrips_via_json() {
        let rule = Rule {
            id: "r1".into(),
            name: "Archive old tmp files".into(),
            enabled: true,
            conditions: ConditionGroup::All(vec![
                ConditionNode::Leaf(Condition::ExtensionIn(vec!["tmp".into(), "log".into()])),
                ConditionNode::Leaf(Condition::CreatedBefore { days_ago: 7 }),
            ]),
            actions: vec![Action::MoveToZone("archive-zone".into())],
            run_mode: RunMode::Interval { minutes: 60 },
            last_run: None,
            run_count: 0,
        };
        let json = serde_json::to_string(&rule).unwrap();
        let parsed: Rule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "r1");
        assert_eq!(parsed.actions.len(), 1);
    }
}
