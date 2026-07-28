//! Restore and reconciliation report model.

use super::*;

// ─── Public report types ────────────────────────────────────────────────

/// Reasons a single restore was skipped (spec G identity ladder).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestoreSkippedReason {
    /// Multiple on-disk files share the same display name and the layout
    /// entry has no `original_path` / `hidden_path` to disambiguate. The
    /// resolver refuses to guess; the caller surfaces a manual-action UI.
    AmbiguousDisplayName,
    /// No on-disk file matches the layout entry by any tier of the ladder.
    Unrecognised,
}

/// One skipped item bubbled up from [`restore_zone_items_with_dirs`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreSkippedItem {
    pub item_id: String,
    pub item_name: String,
    pub reason: RestoreSkippedReason,
}

/// Outcome of [`restore_zone_items_with_dirs`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RestoreZoneItemsReport {
    /// Number of items successfully moved back to their authoritative
    /// on-disk location (Tier 1 / 2 / 3).
    pub restored: u32,
    /// Items the spec G identity ladder refused to restore (Tier 4 / 5).
    pub skipped: Vec<RestoreSkippedItem>,
}

/// Outcome of [`reconcile_zone_items_with_dirs`].
///
/// Same shape as 1.x `ReconcileReport`. The frontend treats it as a status
/// payload — re-fetches `list_zones` when `reconciled_count > 0`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconcileReport {
    /// Items whose physical file was moved from `desktop_dir` into
    /// `.bentodesk/{zone_id}/` during this pass.
    pub reconciled_count: u32,
    /// Items already in `.bentodesk/`. No action taken.
    pub already_managed_count: u32,
    /// Items where neither `hidden_path` nor `original_path` resolves on
    /// disk. Marked `file_missing = true`.
    pub missing_count: u32,
    /// Items with no `original_path` AND no resolvable `hidden_path`.
    /// Counted alongside `missing_count` to avoid silent skips.
    pub unknown_count: u32,
}

/// Mutable item shape consumed by reconcile / restore.
///
/// 1.x reached into `crate::layout::persistence::BentoItem` directly via
/// `AppState`. The native backend does NOT depend on app-layer types per the
/// layer rule (spec §15), so this is a minimal struct with only the fields
/// the stealth subsystem actually reads or writes. The caller (typically
/// `bentodesk-app::backend_bridge`) builds these from its own layout
/// store and writes the mutated `hidden_path` / `file_missing` back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthItem {
    /// Layout-unique item ID.
    pub id: String,
    /// Display name (used by the spec G identity ladder).
    pub name: String,
    /// Where the file originally lived on the user's desktop.
    pub original_path: Option<String>,
    /// Where the file currently lives in `.bentodesk/{zone_id}/`.
    pub hidden_path: Option<String>,
    /// `true` when the resolver could not find the file at any known path.
    pub file_missing: bool,
}
