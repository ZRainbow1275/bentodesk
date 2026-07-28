//! T-088 — Outlook-style Rules engine: data model + persistence.
//!
//! Persisted to `<state_dir>/rules.json` with an atomic write + `.bak`
//! sibling so a crash between rename steps can never lose the rules.
//! Atomic-write + `.bak` is delegated to [`crate::storage`] when the
//! dispatcher composes; this module ships a plain `std::fs` baseline (same
//! shape as `layout::persistence`) so nothing here forces a circular dep
//! on T-090.
//!
//! ## Q2 ruling — regex predicate replaced
//!
//! 1.x had `Condition::NameMatchesRegex(String)`. Per master plan §11 Q2 the
//! `regex` crate is removed from the §8 whitelist and the predicate is
//! split into three fixed-substring forms:
//!
//! - [`Condition::NameStartsWith`]
//! - [`Condition::NameContains`]
//! - [`Condition::NameEndsWith`]
//!
//! **Schema migration (clean break)**: a 1.x `rules.json` carrying
//! `{"type":"NameMatchesRegex","value":"^foo"}` will fail to deserialise.
//! UI/migration code must rewrite to `{"type":"NameStartsWith","value":"foo"}`
//! at upgrade time. There is no compatibility shim — the regex crate is
//! gone and we will not pretend otherwise.
//!
//! ## Tauri removal
//!
//! - `tauri::AppHandle` dropped from every entry; persistence calls take
//!   `state_dir: &Path` instead of `state_data_dir(handle)`.
//! - `app.emit("rule_notification", …)` replaced by a
//!   `crossbeam_channel::Sender<RuleEvent>` parameter on
//!   [`executor::execute`].
//!
//! ## Spec compliance
//!
//! - §8.1 hand-rolled [`RulesError`] (no `thiserror`).
//! - §10 `SmolStr` for short identifiers (`id`, ext patterns, name patterns).
//! - §11 zero `unwrap()` / `expect()` in module body — every `Mutex::lock`
//!   recovers from poisoning via `into_inner()`.
//! - §17 zero `todo!()` / `unimplemented!()`.

pub mod executor;
pub mod scheduler;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the rules persistence module.
#[derive(Debug)]
pub enum RulesError {
    /// `std::fs` read/write/rename failed.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// `serde_json::from_slice` / `serde_json::to_vec` failed.
    Serde {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl core::fmt::Display for RulesError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "rules I/O failed for {}: {source}", path.display())
            }
            Self::Serde { path, source } => {
                write!(f, "rules JSON failed for {}: {source}", path.display())
            }
        }
    }
}

impl core::error::Error for RulesError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Serde { source, .. } => Some(source),
        }
    }
}

// ─── Data model ──────────────────────────────────────────────────────

/// A Rule bundles a condition tree with an ordered action list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub id: SmolStr,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub conditions: ConditionGroup,
    pub actions: Vec<Action>,
    #[serde(default)]
    pub run_mode: RunMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<SmolStr>,
    #[serde(default)]
    pub run_count: u64,
}

fn default_enabled() -> bool {
    true
}

/// Boolean tree of conditions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

/// Either a leaf [`Condition`] or a nested group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ConditionNode {
    Leaf(Condition),
    Group(ConditionGroup),
}

/// Individual file predicate.
///
/// **Q2 ruling**: the 1.x `NameMatchesRegex(String)` variant is replaced by
/// three substring predicates ([`Self::NameStartsWith`],
/// [`Self::NameContains`], [`Self::NameEndsWith`]). The `regex` crate is
/// removed from the §8 whitelist; UI migration must rewrite legacy
/// `^foo`/`foo$` patterns at upgrade time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value")]
pub enum Condition {
    /// Extension is in the provided set (case-insensitive comparison).
    ExtensionIn(Vec<SmolStr>),
    /// Filename begins with this fixed substring (case-sensitive).
    NameStartsWith(SmolStr),
    /// Filename contains this fixed substring (case-sensitive).
    NameContains(SmolStr),
    /// Filename ends with this fixed substring (case-sensitive).
    NameEndsWith(SmolStr),
    /// File was created strictly before `now - days_ago`.
    CreatedBefore { days_ago: u32 },
    /// File was last modified strictly before `now - days_ago`.
    ModifiedBefore { days_ago: u32 },
    /// Size is strictly greater than this many bytes.
    SizeGreaterThan(u64),
    /// File is currently assigned to the given zone id.
    InZone(SmolStr),
    /// File lives directly on the desktop (not in a zone).
    OnDesktop,
}

/// Action to execute on a matched file.
///
/// `MoveToZone` and `Tag` carry layout-mutating semantics that 1.x performed
/// against the global `AppState`. The native executor surfaces those as
/// [`executor::ActionEffect`] for the dispatcher to apply, keeping this
/// crate free of cross-module write coupling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value")]
pub enum Action {
    /// Move / add the file into a specific zone.
    MoveToZone(SmolStr),
    /// Move the raw file to a filesystem folder.
    MoveToFolder(String),
    /// Send to the Recycle Bin.
    DeleteToRecycleBin,
    /// Attach tag(s) to the file (in-layout metadata).
    Tag(Vec<SmolStr>),
    /// Emit a toast notification with a message.
    Notify(String),
}

/// When should the rule run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
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
    pub checkpoint_trigger: SmolStr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_key: Option<SmolStr>,
}

// ─── Persistence ─────────────────────────────────────────────────────

static PERSIST_LOCK: Mutex<()> = Mutex::new(());

fn rules_path(state_dir: &Path) -> PathBuf {
    state_dir.join("rules.json")
}

fn rules_backup_path(state_dir: &Path) -> PathBuf {
    state_dir.join("rules.json.bak")
}

/// Load all persisted rules. Returns an empty list when the file is
/// missing (first launch) or corrupt (matches 1.x `unwrap_or_default`
/// recovery — a broken `rules.json` should not brick the engine).
pub fn load_all(state_dir: &Path) -> Vec<Rule> {
    let path = rules_path(state_dir);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Persist the rule list with `tmp` rename + `.bak` sibling. On failure
/// the previous `rules.json` is preserved at `.bak` so callers can still
/// recover.
pub fn save_all(state_dir: &Path, rules: &[Rule]) -> Result<(), RulesError> {
    // Recover from poisoning — the lock guards a no-op `()`, so the
    // panic-on-prior-write that poisoned it does not mean the on-disk file
    // is in a broken state.
    let _g = PERSIST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    if !state_dir.exists() {
        std::fs::create_dir_all(state_dir).map_err(|e| RulesError::Io {
            path: state_dir.to_path_buf(),
            source: e,
        })?;
    }

    let path = rules_path(state_dir);
    let tmp = path.with_extension("json.tmp");
    let backup = rules_backup_path(state_dir);

    let json = serde_json::to_vec_pretty(rules).map_err(|e| RulesError::Serde {
        path: path.clone(),
        source: e,
    })?;
    std::fs::write(&tmp, &json).map_err(|e| RulesError::Io {
        path: tmp.clone(),
        source: e,
    })?;

    if path.exists() {
        let _ = std::fs::copy(&path, &backup);
    }
    std::fs::rename(&tmp, &path).map_err(|e| RulesError::Io {
        path: path.clone(),
        source: e,
    })?;
    Ok(())
}

/// Insert or replace a rule by id.
pub fn upsert(state_dir: &Path, rule: Rule) -> Result<(), RulesError> {
    let mut rules = load_all(state_dir);
    if let Some(existing) = rules.iter_mut().find(|r| r.id == rule.id) {
        *existing = rule;
    } else {
        rules.push(rule);
    }
    save_all(state_dir, &rules)
}

/// Remove a rule by id.
pub fn delete(state_dir: &Path, id: &str) -> Result<(), RulesError> {
    let mut rules = load_all(state_dir);
    rules.retain(|r| r.id.as_str() != id);
    save_all(state_dir, &rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn scratch_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let tid = std::thread::current().id();
        let path = std::env::temp_dir().join(format!("bentodesk-rules-{tid:?}-{n}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn sample_rule(id: &str) -> Rule {
        Rule {
            id: SmolStr::from(id),
            name: "Archive old tmp files".to_string(),
            enabled: true,
            conditions: ConditionGroup::All(vec![
                ConditionNode::Leaf(Condition::ExtensionIn(vec![
                    SmolStr::new_static("tmp"),
                    SmolStr::new_static("log"),
                ])),
                ConditionNode::Leaf(Condition::CreatedBefore { days_ago: 7 }),
            ]),
            actions: vec![Action::MoveToZone(SmolStr::new_static("archive"))],
            run_mode: RunMode::Interval { minutes: 60 },
            last_run: None,
            run_count: 0,
        }
    }

    #[test]
    fn condition_group_defaults_to_all() {
        match ConditionGroup::default() {
            ConditionGroup::All(v) => assert!(v.is_empty()),
            _ => panic!("expected All"),
        }
    }

    #[test]
    fn rule_round_trips_via_json() {
        let rule = sample_rule("r1");
        let json = serde_json::to_string(&rule).expect("ser");
        let parsed: Rule = serde_json::from_str(&json).expect("de");
        assert_eq!(parsed, rule);
    }

    #[test]
    fn name_predicate_serde_round_trip() {
        let cond = Condition::NameStartsWith(SmolStr::new_static("invoice-"));
        let json = serde_json::to_string(&cond).expect("ser");
        assert!(json.contains("\"NameStartsWith\""));
        assert!(json.contains("\"invoice-\""));
        let back: Condition = serde_json::from_str(&json).expect("de");
        assert_eq!(back, cond);
    }

    #[test]
    fn legacy_regex_predicate_fails_to_deserialise() {
        // Documents the Q2 clean-break — UI migration is mandatory.
        let json = r#"{"type":"NameMatchesRegex","value":"^foo"}"#;
        let result: Result<Condition, _> = serde_json::from_str(json);
        assert!(result.is_err(), "regex variant must NOT round-trip");
    }

    #[test]
    fn upsert_and_load_round_trip() {
        let dir = scratch_dir();
        let rule = sample_rule("r-upsert");
        upsert(&dir, rule.clone()).expect("upsert");
        let loaded = load_all(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], rule);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_replaces_existing_by_id() {
        let dir = scratch_dir();
        let mut rule = sample_rule("r-x");
        upsert(&dir, rule.clone()).expect("first");
        rule.name = "Renamed".to_string();
        upsert(&dir, rule.clone()).expect("second");
        let loaded = load_all(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Renamed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_rule() {
        let dir = scratch_dir();
        upsert(&dir, sample_rule("a")).expect("a");
        upsert(&dir, sample_rule("b")).expect("b");
        delete(&dir, "a").expect("delete");
        let loaded = load_all(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id.as_str(), "b");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_returns_empty_for_missing_file() {
        let dir = scratch_dir().join("nope");
        assert!(load_all(&dir).is_empty());
    }

    #[test]
    fn save_creates_backup_when_overwriting() {
        let dir = scratch_dir();
        upsert(&dir, sample_rule("seed")).expect("seed");
        upsert(&dir, sample_rule("second")).expect("second");
        assert!(
            rules_backup_path(&dir).exists(),
            ".bak should be created on overwrite"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
