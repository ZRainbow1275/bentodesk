//! T-087 — desktop file scanner + smart auto-grouping.
//!
//! Scans the user's Desktop directory, builds language-agnostic feature
//! vectors, runs single-pass hierarchical clustering, and emits ranked
//! [`SuggestedGroup`]s the UI can apply with one click.
//!
//! ## Sub-modules
//!
//! - [`scanner`] — `FileInfo` + `scan_desktop_files`. Plain `std::fs`
//!   walk, RFC3339 timestamps via [`crate::time`].
//! - [`feature`] — token + similarity primitives (Jaccard, weighted
//!   composite).
//! - [`rules`] — `EXTENSION_GROUPS` constant + `date_group_name` for
//!   relative-time bucketing.
//! - [`suggestions`] — orchestrates extension / prefix / AI heuristics into
//!   a ranked list.
//! - [`ai_recommender`] — single-pass hierarchical clustering on top of
//!   `feature::FeatureVector`.
//!
//! ## What changed vs 1.x
//!
//! - **Q1**: every `chrono::DateTime::parse_from_rfc3339(iso).timestamp()`
//!   call goes through [`crate::time::parse_rfc3339_to_unix_secs`].
//!   `chrono` is no longer in the §8 whitelist.
//! - **Q2 cleanup**: `ai_recommender` and `suggestions` no longer emit
//!   `AutoGroupRule.pattern` with the legacy `^prefix` regex anchor — the
//!   pattern is a bare lowercase prefix string. The `regex_escape` helper
//!   from 1.x is gone.
//! - **Tauri** removal: every entry takes plain `&[FileInfo]` /
//!   `&Path` / `&[BentoZone]` parameters; no `AppHandle`.

pub mod ai_recommender;
pub mod feature;
pub mod rules;
pub mod scanner;
pub mod suggestions;

pub use scanner::{FileInfo, scan_desktop_files};
pub use suggestions::{SuggestedGroup, suggest_groups};

use std::borrow::Cow;

use bento_nano_zone::{Zone, ZoneId, ZoneList};

/// Default geometry for a freshly applied auto-group zone.
///
/// F1 placement contract: spawn at viewport-origin with a 200×120 DIP card
/// footprint. The shell handler is responsible for any post-creation
/// re-centring once the viewport size is known (kept out of this layer to
/// preserve the §10 zero-platform-coupling rule on `bento-nano-backend`).
const APPLY_DEFAULT_X: i32 = 0;
const APPLY_DEFAULT_Y: i32 = 0;
const APPLY_DEFAULT_W: i32 = 200;
const APPLY_DEFAULT_H: i32 = 120;

/// Errors surfaced by `apply_auto_group`.
///
/// Hand-rolled per spec §8.1 (no `thiserror`). Reserved variant `IdOverflow`
/// fires only if the existing zone list has saturated `u64::MAX` ids — kept
/// as an explicit error rather than `panic!` to honour spec §11 no-panic.
#[derive(Debug)]
pub enum GroupingError {
    /// Monotonic id allocator wrapped past `u64::MAX`. Practically
    /// unreachable (would require ~1.8e19 zones across product lifetime),
    /// but required so the API never panics.
    IdOverflow,
}

impl core::fmt::Display for GroupingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::IdOverflow => write!(f, "zone id allocator exhausted (u64 saturated)"),
        }
    }
}

impl core::error::Error for GroupingError {}

/// Apply a `SuggestedGroup` by minting a new zone in `zones` and returning
/// its [`ZoneId`].
///
/// Allocator strategy: derives the next id from `max(existing) + 1`, so the
/// function is self-contained and doesn't reach back into `AppState`'s
/// `next_zone_id` cell. The shell-layer dispatcher arm wraps the call inside
/// a `RefMut<AppState>` borrow and may post-adjust `app.next_zone_id` to
/// stay monotonic with this output.
///
/// F5 scope: matched files are materialised into the zone's item list using
/// their real filesystem paths. Icon hashes start empty here because the shell
/// layer owns the Win32 icon-cache handle and warms icons after applying the
/// group; the item path remains the source of truth if icon extraction fails.
pub fn apply_auto_group(
    suggestion: &SuggestedGroup,
    zones: &mut ZoneList,
) -> Result<ZoneId, GroupingError> {
    let next_raw = zones
        .iter()
        .map(|z| z.id.0)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(GroupingError::IdOverflow)?;
    let id = ZoneId(next_raw);

    let mut zone = Zone::new(
        id,
        Cow::Owned(suggestion.name.clone()),
        APPLY_DEFAULT_X,
        APPLY_DEFAULT_Y,
        APPLY_DEFAULT_W,
        APPLY_DEFAULT_H,
    );
    for path in &suggestion.matching_files {
        let _ = zone.add_item(Cow::Owned(path.clone()), Cow::Borrowed(""));
    }
    zones.add(zone);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{AutoGroupRule, GroupRuleType};

    fn sample_suggestion(name: &str) -> SuggestedGroup {
        SuggestedGroup {
            name: name.to_string(),
            icon: "\u{1F4C1}".to_string(),
            rule: AutoGroupRule {
                rule_type: GroupRuleType::Extension,
                pattern: None,
                extensions: Some(vec!["pdf".to_string()]),
            },
            matching_files: vec![
                "C:\\Desktop\\a.pdf".to_string(),
                "C:\\Desktop\\b.pdf".to_string(),
                "C:\\Desktop\\c.pdf".to_string(),
            ],
            confidence: 0.75,
        }
    }

    #[test]
    fn apply_auto_group_creates_zone_and_returns_id() {
        let mut zones = ZoneList::new();
        let suggestion = sample_suggestion("Documents");

        let id = apply_auto_group(&suggestion, &mut zones).expect("apply should succeed");

        assert_eq!(zones.len(), 1, "ZoneList must grow by exactly one");
        let z = zones.get(id).expect("returned id must resolve in ZoneList");
        assert_eq!(z.title.as_ref(), "Documents");
        assert_eq!(z.w, APPLY_DEFAULT_W);
        assert_eq!(z.h, APPLY_DEFAULT_H);
        assert_eq!(z.items.len(), 3);
        assert_eq!(z.items[0].path.as_ref(), "C:\\Desktop\\a.pdf");
    }

    #[test]
    fn apply_auto_group_id_is_monotonic_above_existing_max() {
        let mut zones = ZoneList::new();
        zones.add(Zone::new(ZoneId(7), Cow::Borrowed("seed"), 0, 0, 100, 100));
        zones.add(Zone::new(ZoneId(3), Cow::Borrowed("seed2"), 0, 0, 100, 100));

        let id = apply_auto_group(&sample_suggestion("Images"), &mut zones)
            .expect("apply should succeed");

        assert_eq!(
            id,
            ZoneId(8),
            "next id must be max(existing)+1, not list-len"
        );
        assert_eq!(zones.len(), 3);
    }

    #[test]
    fn apply_auto_group_on_empty_list_starts_at_one() {
        let mut zones = ZoneList::new();
        let id = apply_auto_group(&sample_suggestion("First"), &mut zones)
            .expect("apply should succeed");
        assert_eq!(
            id,
            ZoneId(1),
            "first allocation must skip ZoneId::INVALID (0)"
        );
    }
}
