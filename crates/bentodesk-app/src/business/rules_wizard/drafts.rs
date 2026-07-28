//! Draft types backing the wizard form — each step's user-editable buffer
//! plus the small dictionaries (`PredicateKind`, `ActionKind`, etc.) that
//! drive the step UI.
//!
//! Everything in here is **representation**: what the user is typing right
//! now, plus the reverse mapping from a saved [`Rule`] back into draft form
//! when the wizard opens in Edit mode. Construction of the final
//! [`Rule`] lives in `mod.rs::RulesWizardState::build_rule`.

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use bentodesk_backend::rules::{Action, Condition, ConditionGroup, ConditionNode};

// -----------------------------------------------------------------------------
// PredicateKind — discriminator for the step-1 condition row dropdown.
// Kept separate from `Condition` so the dropdown can list the kinds without
// constructing a placeholder value for every variant (which would force
// callers to invent a `SmolStr` payload they don't have yet).
// -----------------------------------------------------------------------------

/// Discriminator for the supported leaf-condition variants. Q2 anchor-free —
/// no regex variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PredicateKind {
    /// Filename starts with a literal substring.
    #[default]
    NameStartsWith,
    /// Filename contains a literal substring.
    NameContains,
    /// Filename ends with a literal substring.
    NameEndsWith,
    /// Extension is in a comma-separated set.
    ExtensionIn,
    /// File was created strictly more than N days ago.
    CreatedBefore,
    /// File was last modified strictly more than N days ago.
    ModifiedBefore,
    /// File size is strictly greater than N bytes.
    SizeGreaterThan,
    /// File is currently assigned to the named zone.
    InZone,
    /// File lives directly on the desktop.
    OnDesktop,
}

impl PredicateKind {
    /// Stable iteration order matching the dropdown row layout.
    pub const ALL: &'static [Self] = &[
        Self::NameStartsWith,
        Self::NameContains,
        Self::NameEndsWith,
        Self::ExtensionIn,
        Self::CreatedBefore,
        Self::ModifiedBefore,
        Self::SizeGreaterThan,
        Self::InZone,
        Self::OnDesktop,
    ];

    /// Whether this predicate carries an inline value input.
    pub const fn needs_value(self) -> bool {
        !matches!(self, Self::OnDesktop)
    }
}

/// Editable condition row — a kind + a single inline value (parsed against
/// the kind on commit). Storing the raw user text lets the row reject
/// invalid input at the boundary (e.g. non-numeric `days_ago`) without
/// dropping intermediate keystrokes the user is still typing.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionDraft {
    pub kind: PredicateKind,
    /// Raw text the user typed. Parsed into the appropriate `Condition`
    /// variant on commit; ignored when [`PredicateKind::needs_value`] is
    /// false. Plain `String` because the user can type arbitrary content
    /// (extensions, names, zone ids) of any length.
    pub value: String,
}

impl ConditionDraft {
    /// Default draft — one [`PredicateKind::NameStartsWith`] row with empty
    /// value.
    pub fn new() -> Self {
        Self {
            kind: PredicateKind::default(),
            value: String::new(),
        }
    }

    /// Whether the draft is well-formed enough to ship into a `Condition`.
    pub fn is_valid(&self) -> bool {
        if !self.kind.needs_value() {
            return true;
        }
        let trimmed = self.value.trim();
        if trimmed.is_empty() {
            return false;
        }
        match self.kind {
            PredicateKind::CreatedBefore | PredicateKind::ModifiedBefore => {
                trimmed.parse::<u32>().is_ok()
            }
            PredicateKind::SizeGreaterThan => trimmed.parse::<u64>().is_ok(),
            _ => true,
        }
    }

    /// Convert to a backend `Condition` variant. Returns `None` when the
    /// draft fails [`is_valid`].
    ///
    /// [`is_valid`]: ConditionDraft::is_valid
    pub fn to_condition(&self) -> Option<Condition> {
        if !self.is_valid() {
            return None;
        }
        let trimmed = self.value.trim();
        match self.kind {
            PredicateKind::NameStartsWith => Some(Condition::NameStartsWith(SmolStr::new(trimmed))),
            PredicateKind::NameContains => Some(Condition::NameContains(SmolStr::new(trimmed))),
            PredicateKind::NameEndsWith => Some(Condition::NameEndsWith(SmolStr::new(trimmed))),
            PredicateKind::ExtensionIn => {
                let exts: Vec<SmolStr> = trimmed
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(SmolStr::new)
                    .collect();
                if exts.is_empty() {
                    None
                } else {
                    Some(Condition::ExtensionIn(exts))
                }
            }
            PredicateKind::CreatedBefore => trimmed
                .parse::<u32>()
                .ok()
                .map(|days_ago| Condition::CreatedBefore { days_ago }),
            PredicateKind::ModifiedBefore => trimmed
                .parse::<u32>()
                .ok()
                .map(|days_ago| Condition::ModifiedBefore { days_ago }),
            PredicateKind::SizeGreaterThan => {
                trimmed.parse::<u64>().ok().map(Condition::SizeGreaterThan)
            }
            PredicateKind::InZone => Some(Condition::InZone(SmolStr::new(trimmed))),
            PredicateKind::OnDesktop => Some(Condition::OnDesktop),
        }
    }
}

impl Default for ConditionDraft {
    fn default() -> Self {
        Self::new()
    }
}

/// Top-level boolean combinator chosen on step 1. Maps to
/// `ConditionGroup::All` / `ConditionGroup::Any` on commit. `Not` isn't
/// surfaced by the wizard — it's for advanced authoring through the JSON
/// editor only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CombineMode {
    /// All conditions must match.
    #[default]
    All,
    /// Any condition matches.
    Any,
}

// -----------------------------------------------------------------------------
// ActionKind — discriminator for the step-2 action-card row.
// -----------------------------------------------------------------------------

/// Discriminator for the supported `Action` variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Move matched files into a specific zone.
    #[default]
    MoveToZone,
    /// Move matched files to a filesystem folder.
    MoveToFolder,
    /// Send matched files to the Recycle Bin.
    DeleteToRecycleBin,
    /// Attach tag(s) to matched files.
    Tag,
    /// Emit a toast notification.
    Notify,
}

impl ActionKind {
    /// Iteration order for the action card row.
    pub const ALL: &'static [Self] = &[
        Self::MoveToZone,
        Self::MoveToFolder,
        Self::DeleteToRecycleBin,
        Self::Tag,
        Self::Notify,
    ];
}

/// Editable action draft — a kind + a single inline value (zone id /
/// folder path / comma-separated tags / notify message), unused for
/// `DeleteToRecycleBin`.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionDraft {
    pub kind: ActionKind,
    pub value: String,
}

impl ActionDraft {
    pub fn new() -> Self {
        Self {
            kind: ActionKind::default(),
            value: String::new(),
        }
    }

    /// Whether the draft is well-formed enough to ship into an `Action`.
    pub fn is_valid(&self) -> bool {
        match self.kind {
            ActionKind::DeleteToRecycleBin => true,
            _ => !self.value.trim().is_empty(),
        }
    }

    /// Convert to a backend `Action` variant. Returns `None` when the draft
    /// fails [`is_valid`].
    ///
    /// [`is_valid`]: ActionDraft::is_valid
    pub fn to_action(&self) -> Option<Action> {
        if !self.is_valid() {
            return None;
        }
        let trimmed = self.value.trim();
        match self.kind {
            ActionKind::MoveToZone => Some(Action::MoveToZone(SmolStr::new(trimmed))),
            ActionKind::MoveToFolder => Some(Action::MoveToFolder(trimmed.to_string())),
            ActionKind::DeleteToRecycleBin => Some(Action::DeleteToRecycleBin),
            ActionKind::Tag => {
                let tags: Vec<SmolStr> = trimmed
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(SmolStr::new)
                    .collect();
                if tags.is_empty() {
                    None
                } else {
                    Some(Action::Tag(tags))
                }
            }
            ActionKind::Notify => Some(Action::Notify(trimmed.to_string())),
        }
    }
}

impl Default for ActionDraft {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// RunModeChoice — segmented switch on step 4. Mirrors backend `RunMode`.
// -----------------------------------------------------------------------------

/// Discriminator for the supported `RunMode` variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunModeChoice {
    #[default]
    OnDemand,
    OnFileChange,
    Interval,
}

impl RunModeChoice {
    /// Iteration order for the run-mode radio row.
    pub const ALL: &'static [Self] = &[Self::OnDemand, Self::OnFileChange, Self::Interval];
}

// -----------------------------------------------------------------------------
// Conversion helpers — backend → draft (Edit-mode load).
// -----------------------------------------------------------------------------

/// Decompose a `ConditionGroup` into a `(CombineMode, Vec<ConditionDraft>)`.
/// Nested groups and `Not` wrappers flatten to the closest leaf list — the
/// wizard doesn't surface them so an Edit-mode load that hits a nested
/// shape collapses to the visible leaves the user can manage.
pub(super) fn decompose_conditions(group: &ConditionGroup) -> (CombineMode, Vec<ConditionDraft>) {
    match group {
        ConditionGroup::All(nodes) => (CombineMode::All, flatten_nodes(nodes)),
        ConditionGroup::Any(nodes) => (CombineMode::Any, flatten_nodes(nodes)),
        ConditionGroup::Not(inner) => {
            // Wizard can't represent NOT — fall back to All of the inner
            // leaves. The shell logs this fall-back so an advanced user
            // editing through the JSON editor sees the warning.
            let (_, drafts) = decompose_conditions(inner);
            (CombineMode::All, drafts)
        }
    }
}

fn flatten_nodes(nodes: &[ConditionNode]) -> Vec<ConditionDraft> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            ConditionNode::Leaf(c) => out.push(condition_to_draft(c)),
            ConditionNode::Group(g) => {
                let (_, nested) = decompose_conditions(g);
                out.extend(nested);
            }
        }
    }
    out
}

fn condition_to_draft(c: &Condition) -> ConditionDraft {
    match c {
        Condition::NameStartsWith(s) => ConditionDraft {
            kind: PredicateKind::NameStartsWith,
            value: s.to_string(),
        },
        Condition::NameContains(s) => ConditionDraft {
            kind: PredicateKind::NameContains,
            value: s.to_string(),
        },
        Condition::NameEndsWith(s) => ConditionDraft {
            kind: PredicateKind::NameEndsWith,
            value: s.to_string(),
        },
        Condition::ExtensionIn(exts) => ConditionDraft {
            kind: PredicateKind::ExtensionIn,
            value: exts
                .iter()
                .map(SmolStr::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        },
        Condition::CreatedBefore { days_ago } => ConditionDraft {
            kind: PredicateKind::CreatedBefore,
            value: days_ago.to_string(),
        },
        Condition::ModifiedBefore { days_ago } => ConditionDraft {
            kind: PredicateKind::ModifiedBefore,
            value: days_ago.to_string(),
        },
        Condition::SizeGreaterThan(bytes) => ConditionDraft {
            kind: PredicateKind::SizeGreaterThan,
            value: bytes.to_string(),
        },
        Condition::InZone(z) => ConditionDraft {
            kind: PredicateKind::InZone,
            value: z.to_string(),
        },
        Condition::OnDesktop => ConditionDraft {
            kind: PredicateKind::OnDesktop,
            value: String::new(),
        },
    }
}

pub(super) fn action_to_draft(action: Action) -> ActionDraft {
    match action {
        Action::MoveToZone(z) => ActionDraft {
            kind: ActionKind::MoveToZone,
            value: z.into(),
        },
        Action::MoveToFolder(f) => ActionDraft {
            kind: ActionKind::MoveToFolder,
            value: f,
        },
        Action::DeleteToRecycleBin => ActionDraft {
            kind: ActionKind::DeleteToRecycleBin,
            value: String::new(),
        },
        Action::Tag(tags) => ActionDraft {
            kind: ActionKind::Tag,
            value: tags
                .iter()
                .map(SmolStr::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        },
        Action::Notify(msg) => ActionDraft {
            kind: ActionKind::Notify,
            value: msg,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── PredicateKind / ConditionDraft ─────────────────────────────

    #[test]
    fn predicate_needs_value_only_false_for_on_desktop() {
        for kind in PredicateKind::ALL {
            let needs = kind.needs_value();
            assert_eq!(needs, !matches!(kind, PredicateKind::OnDesktop));
        }
    }

    #[test]
    fn condition_draft_default_is_invalid() {
        let d = ConditionDraft::new();
        assert!(!d.is_valid(), "empty NameStartsWith value is not valid");
    }

    #[test]
    fn on_desktop_draft_is_always_valid() {
        let d = ConditionDraft {
            kind: PredicateKind::OnDesktop,
            value: String::new(),
        };
        assert!(d.is_valid());
        assert_eq!(d.to_condition(), Some(Condition::OnDesktop));
    }

    #[test]
    fn name_starts_with_to_condition_trims_value() {
        let d = ConditionDraft {
            kind: PredicateKind::NameStartsWith,
            value: "  invoice- ".into(),
        };
        assert_eq!(
            d.to_condition(),
            Some(Condition::NameStartsWith(SmolStr::new_static("invoice-")))
        );
    }

    #[test]
    fn extension_in_to_condition_splits_and_trims() {
        let d = ConditionDraft {
            kind: PredicateKind::ExtensionIn,
            value: " pdf, doc , xls ".into(),
        };
        let cond = d.to_condition().expect("valid");
        match cond {
            Condition::ExtensionIn(exts) => {
                assert_eq!(exts.len(), 3);
                assert_eq!(exts[0].as_str(), "pdf");
                assert_eq!(exts[1].as_str(), "doc");
                assert_eq!(exts[2].as_str(), "xls");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn extension_in_with_only_separators_is_invalid() {
        let d = ConditionDraft {
            kind: PredicateKind::ExtensionIn,
            value: " , , ".into(),
        };
        // is_valid only checks the trimmed-non-empty rule; to_condition is
        // the source of truth for "actually empty after parsing".
        assert!(d.to_condition().is_none());
    }

    #[test]
    fn created_before_with_non_numeric_value_is_invalid() {
        let d = ConditionDraft {
            kind: PredicateKind::CreatedBefore,
            value: "yesterday".into(),
        };
        assert!(!d.is_valid());
        assert!(d.to_condition().is_none());
    }

    #[test]
    fn size_greater_than_to_condition_parses_u64() {
        let d = ConditionDraft {
            kind: PredicateKind::SizeGreaterThan,
            value: "10485760".into(),
        };
        assert_eq!(
            d.to_condition(),
            Some(Condition::SizeGreaterThan(10_485_760))
        );
    }

    // ─── Action draft ───────────────────────────────────────────────

    #[test]
    fn action_draft_default_is_invalid() {
        assert!(!ActionDraft::new().is_valid());
    }

    #[test]
    fn delete_to_recycle_bin_is_always_valid() {
        let d = ActionDraft {
            kind: ActionKind::DeleteToRecycleBin,
            value: String::new(),
        };
        assert!(d.is_valid());
        assert_eq!(d.to_action(), Some(Action::DeleteToRecycleBin));
    }

    #[test]
    fn tag_draft_splits_comma_separated() {
        let d = ActionDraft {
            kind: ActionKind::Tag,
            value: " urgent, work ".into(),
        };
        let action = d.to_action().expect("valid");
        match action {
            Action::Tag(tags) => {
                assert_eq!(tags.len(), 2);
                assert_eq!(tags[0].as_str(), "urgent");
                assert_eq!(tags[1].as_str(), "work");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn move_to_folder_draft_keeps_path_verbatim() {
        let d = ActionDraft {
            kind: ActionKind::MoveToFolder,
            value: " C:/Users/X1/Archive ".into(),
        };
        match d.to_action().expect("valid") {
            Action::MoveToFolder(folder) => assert_eq!(folder, "C:/Users/X1/Archive"),
            _ => panic!("wrong variant"),
        }
    }

    // ─── Conversion helpers ────────────────────────────────────────

    #[test]
    fn decompose_all_group_returns_all_combine_mode() {
        let g = ConditionGroup::All(vec![ConditionNode::Leaf(Condition::OnDesktop)]);
        let (mode, drafts) = decompose_conditions(&g);
        assert_eq!(mode, CombineMode::All);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].kind, PredicateKind::OnDesktop);
    }

    #[test]
    fn decompose_any_group_returns_any_combine_mode() {
        let g = ConditionGroup::Any(vec![ConditionNode::Leaf(Condition::NameStartsWith(
            SmolStr::new_static("inv"),
        ))]);
        let (mode, drafts) = decompose_conditions(&g);
        assert_eq!(mode, CombineMode::Any);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].kind, PredicateKind::NameStartsWith);
        assert_eq!(drafts[0].value, "inv");
    }

    #[test]
    fn decompose_not_falls_back_to_all_of_inner_leaves() {
        let g = ConditionGroup::Not(Box::new(ConditionGroup::Any(vec![ConditionNode::Leaf(
            Condition::OnDesktop,
        )])));
        let (mode, drafts) = decompose_conditions(&g);
        assert_eq!(mode, CombineMode::All);
        assert_eq!(drafts.len(), 1);
    }

    #[test]
    fn decompose_nested_group_flattens_inner_leaves() {
        let inner = ConditionGroup::Any(vec![ConditionNode::Leaf(Condition::OnDesktop)]);
        let g = ConditionGroup::All(vec![
            ConditionNode::Leaf(Condition::NameContains(SmolStr::new_static("x"))),
            ConditionNode::Group(inner),
        ]);
        let (mode, drafts) = decompose_conditions(&g);
        assert_eq!(mode, CombineMode::All);
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].kind, PredicateKind::NameContains);
        assert_eq!(drafts[1].kind, PredicateKind::OnDesktop);
    }

    #[test]
    fn action_to_draft_round_trips_every_variant() {
        let pairs = [
            (
                Action::MoveToZone(SmolStr::new_static("inbox")),
                ActionKind::MoveToZone,
                "inbox",
            ),
            (
                Action::MoveToFolder("C:/Archive".into()),
                ActionKind::MoveToFolder,
                "C:/Archive",
            ),
            (
                Action::DeleteToRecycleBin,
                ActionKind::DeleteToRecycleBin,
                "",
            ),
            (
                Action::Tag(vec![SmolStr::new_static("a"), SmolStr::new_static("b")]),
                ActionKind::Tag,
                "a, b",
            ),
            (Action::Notify("hello".into()), ActionKind::Notify, "hello"),
        ];
        for (action, expected_kind, expected_value) in pairs {
            let draft = action_to_draft(action);
            assert_eq!(draft.kind, expected_kind);
            assert_eq!(draft.value, expected_value);
        }
    }
}
