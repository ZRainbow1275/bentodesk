//! T-089 — checkpoint model + on-disk store for the time-machine timeline.
//!
//! A [`Checkpoint`] reuses the existing [`crate::layout::DesktopSnapshot`] to
//! record the full zone set at a point in time, and augments it with a
//! human-readable delta summary and the trigger that caused the capture.
//! Storage is a flat directory of `checkpoint-{timestamp}.json` files under
//! `<state_dir>/timeline/`.
//!
//! ## What changed vs 1.x
//!
//! - `chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ")` replaced by
//!   [`crate::time::now_compact_rfc3339`] (Q1).
//! - `uuid::Uuid::new_v4().to_string()[..8]` replaced by an epoch-nanos
//!   hex suffix; the `uuid` crate is not on the §8 whitelist. Collision
//!   probability inside a single nanosecond per process is zero.
//! - `crate::error::BentoDeskError` replaced by hand-rolled
//!   [`CheckpointError`] (spec §8.1).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::layout::{BentoZone, DesktopSnapshot};
use crate::time;

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the checkpoint store.
#[derive(Debug)]
pub enum CheckpointError {
    /// `std::fs` read/write/remove failed.
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

impl core::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "checkpoint I/O failed for {}: {source}", path.display())
            }
            Self::Serde { path, source } => {
                write!(f, "checkpoint JSON failed for {}: {source}", path.display())
            }
        }
    }
}

impl core::error::Error for CheckpointError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Serde { source, .. } => Some(source),
        }
    }
}

// ─── DeltaSummary ────────────────────────────────────────────────────

/// A structured diff description between two checkpoints, used both for
/// the slider tooltip (`"+3 items, -1 zone"`) and for the
/// `on_significant_change` threshold decision.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeltaSummary {
    #[serde(default)]
    pub items_added: i32,
    #[serde(default)]
    pub items_removed: i32,
    #[serde(default)]
    pub items_moved: i32,
    #[serde(default)]
    pub zones_added: i32,
    #[serde(default)]
    pub zones_removed: i32,
    #[serde(default)]
    pub zones_updated: i32,
}

impl DeltaSummary {
    /// Total number of item-level changes (add+remove+move).
    pub fn item_churn(&self) -> i32 {
        self.items_added + self.items_removed + self.items_moved
    }

    /// Total number of zone-level changes (add+remove+update).
    pub fn zone_churn(&self) -> i32 {
        self.zones_added + self.zones_removed + self.zones_updated
    }

    /// Render a compact human summary, e.g. `"+3 items, -1 zone"`.
    pub fn human(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.items_added > 0 {
            parts.push(format!("+{} items", self.items_added));
        }
        if self.items_removed > 0 {
            parts.push(format!("-{} items", self.items_removed));
        }
        if self.items_moved > 0 {
            parts.push(format!("~{} moved", self.items_moved));
        }
        if self.zones_added > 0 {
            parts.push(format!("+{} zones", self.zones_added));
        }
        if self.zones_removed > 0 {
            parts.push(format!("-{} zones", self.zones_removed));
        }
        if self.zones_updated > 0 && self.zones_added == 0 && self.zones_removed == 0 {
            parts.push(format!("~{} zones", self.zones_updated));
        }
        if parts.is_empty() {
            "no change".to_string()
        } else {
            parts.join(", ")
        }
    }
}

// ─── Checkpoint + CheckpointMeta ─────────────────────────────────────

/// The on-disk checkpoint record.
///
/// `id` is the file-safe timestamp (e.g. `20260417T120501123Z-a1b2c3d4`)
/// also used as the directory entry stem so listing can sort
/// lexicographically.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Checkpoint {
    pub id: SmolStr,
    pub snapshot: DesktopSnapshot,
    #[serde(default)]
    pub delta: DeltaSummary,
    #[serde(default)]
    pub delta_summary: String,
    #[serde(default)]
    pub trigger: SmolStr,
    /// Stable burst key used by selected-stack callers that need the same
    /// debounce/coalescing semantics as the old Tauri hook without requiring
    /// a Tauri `AppHandle`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coalesce_key: Option<SmolStr>,
    /// Manual / pinned checkpoints are never evicted by the ring buffer.
    #[serde(default)]
    pub pinned: bool,
}

/// Lightweight metadata sent over the dispatcher's command bus — avoids
/// shipping full zone data in the initial `list_checkpoints` payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointMeta {
    pub id: SmolStr,
    pub captured_at: SmolStr,
    pub trigger: SmolStr,
    pub delta_summary: String,
    pub pinned: bool,
    pub zone_count: usize,
    pub item_count: usize,
}

impl From<&Checkpoint> for CheckpointMeta {
    fn from(cp: &Checkpoint) -> Self {
        let item_count = cp
            .snapshot
            .zones
            .iter()
            .map(|z| z.items.len())
            .sum::<usize>();
        Self {
            id: cp.id.clone(),
            captured_at: cp.snapshot.captured_at.clone(),
            trigger: cp.trigger.clone(),
            delta_summary: cp.delta_summary.clone(),
            pinned: cp.pinned,
            zone_count: cp.snapshot.zones.len(),
            item_count,
        }
    }
}

// ─── CheckpointStore ─────────────────────────────────────────────────

/// Disk-backed checkpoint store.
pub struct CheckpointStore {
    dir: PathBuf,
}

impl CheckpointStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn save(&self, cp: &Checkpoint) -> Result<(), CheckpointError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| CheckpointError::Io {
            path: self.dir.clone(),
            source: e,
        })?;
        let path = self.path_for(&cp.id);
        let bytes = serde_json::to_vec(cp).map_err(|e| CheckpointError::Serde {
            path: path.clone(),
            source: e,
        })?;
        std::fs::write(&path, &bytes).map_err(|e| CheckpointError::Io {
            path: path.clone(),
            source: e,
        })
    }

    pub fn load(&self, id: &str) -> Result<Checkpoint, CheckpointError> {
        let path = self.path_for(id);
        let bytes = std::fs::read(&path).map_err(|e| CheckpointError::Io {
            path: path.clone(),
            source: e,
        })?;
        serde_json::from_slice(&bytes).map_err(|e| CheckpointError::Serde {
            path: path.clone(),
            source: e,
        })
    }

    /// Idempotent — no error when the file is already absent.
    pub fn delete(&self, id: &str) -> Result<(), CheckpointError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Ok(());
        }
        std::fs::remove_file(&path).map_err(|e| CheckpointError::Io {
            path: path.clone(),
            source: e,
        })
    }

    /// Load every checkpoint, sorted ascending by `captured_at`. Invalid
    /// files are skipped with a warning so a single bad checkpoint does
    /// not poison the list (1.x behaviour).
    pub fn load_all(&self) -> Vec<Checkpoint> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(_) => return out,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(err) => {
                    tracing::warn!(
                        "Timeline: skipping unreadable checkpoint {}: {err}",
                        path.display()
                    );
                    continue;
                }
            };
            match serde_json::from_slice::<Checkpoint>(&bytes) {
                Ok(cp) => out.push(cp),
                Err(err) => {
                    tracing::warn!(
                        "Timeline: skipping corrupt checkpoint {}: {err}",
                        path.display()
                    );
                }
            }
        }
        out.sort_by(|a, b| a.snapshot.captured_at.cmp(&b.snapshot.captured_at));
        out
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("checkpoint-{id}.json"))
    }
}

// ─── id helper ───────────────────────────────────────────────────────

/// Generate a file-safe, lexicographically-sortable checkpoint id.
///
/// Format: `YYYYMMDDTHHMMSSmmmZ-XXXXXXXX` where the suffix is the lower
/// 32 bits of `SystemTime::now()`'s subsecond nanos rendered as 8-char
/// hex. Collisions inside a single nanosecond per process are zero.
pub fn new_checkpoint_id() -> SmolStr {
    let ts = time::now_compact_rfc3339();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    SmolStr::from(format!("{ts}-{nanos:08x}"))
}

// ─── Diff ────────────────────────────────────────────────────────────

/// Compute the delta between a previous snapshot and the current zones.
pub fn compute_delta(prev: Option<&DesktopSnapshot>, current_zones: &[BentoZone]) -> DeltaSummary {
    let mut d = DeltaSummary::default();
    let prev_zones = prev.map(|s| s.zones.as_slice()).unwrap_or(&[]);

    let prev_ids: HashSet<&str> = prev_zones.iter().map(|z| z.id.as_str()).collect();
    let cur_ids: HashSet<&str> = current_zones.iter().map(|z| z.id.as_str()).collect();

    d.zones_added = cur_ids.difference(&prev_ids).count() as i32;
    d.zones_removed = prev_ids.difference(&cur_ids).count() as i32;

    let prev_item_ids: HashSet<&str> = prev_zones
        .iter()
        .flat_map(|z| z.items.iter().map(|i| i.id.as_str()))
        .collect();
    let cur_item_ids: HashSet<&str> = current_zones
        .iter()
        .flat_map(|z| z.items.iter().map(|i| i.id.as_str()))
        .collect();
    d.items_added = cur_item_ids.difference(&prev_item_ids).count() as i32;
    d.items_removed = prev_item_ids.difference(&cur_item_ids).count() as i32;

    let prev_item_zone: HashMap<&str, &str> = prev_zones
        .iter()
        .flat_map(|z| z.items.iter().map(move |i| (i.id.as_str(), z.id.as_str())))
        .collect();
    for cur in current_zones {
        for item in &cur.items {
            if let Some(&prev_zone_id) = prev_item_zone.get(item.id.as_str())
                && prev_zone_id != cur.id.as_str()
            {
                d.items_moved += 1;
            }
        }
        if let Some(prev) = prev_zones.iter().find(|z| z.id == cur.id)
            && zone_metadata_changed(prev, cur)
        {
            d.zones_updated += 1;
        }
    }

    d
}

fn zone_metadata_changed(prev: &BentoZone, current: &BentoZone) -> bool {
    prev.name != current.name
        || prev.icon != current.icon
        || prev.position != current.position
        || prev.expanded_size != current.expanded_size
        || prev.accent_color != current.accent_color
        || prev.sort_order != current.sort_order
        || prev.auto_group != current.auto_group
        || prev.grid_columns != current.grid_columns
        || prev.capsule_size != current.capsule_size
        || prev.capsule_shape != current.capsule_shape
        || prev.locked != current.locked
        || prev.visible != current.visible
        || prev.stack_id != current.stack_id
        || prev.stack_order != current.stack_order
        || prev.alias != current.alias
        || prev.display_mode != current.display_mode
        || prev.live_folder_path != current.live_folder_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{
        BentoItem, BentoZone, GridPosition, ItemType, RelativePosition, RelativeSize, Resolution,
    };
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn scratch_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let tid = std::thread::current().id();
        let path = std::env::temp_dir().join(format!("bentodesk-cp-{tid:?}-{n}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn make_zone(id: &str, items: Vec<&str>) -> BentoZone {
        BentoZone {
            id: SmolStr::from(id),
            name: format!("Z-{id}"),
            icon: SmolStr::new_static("folder"),
            position: RelativePosition {
                x_percent: 0.0,
                y_percent: 0.0,
            },
            expanded_size: RelativeSize {
                w_percent: 30.0,
                h_percent: 30.0,
            },
            items: items
                .into_iter()
                .map(|iid| BentoItem {
                    id: SmolStr::from(iid),
                    zone_id: SmolStr::from(id),
                    item_type: ItemType::File,
                    name: iid.to_string(),
                    path: format!("C:/{iid}"),
                    icon_hash: SmolStr::new_static(""),
                    grid_position: GridPosition {
                        col: 0,
                        row: 0,
                        col_span: 1,
                    },
                    is_wide: false,
                    added_at: SmolStr::new_static(""),
                    original_path: None,
                    hidden_path: None,
                    icon_x: None,
                    icon_y: None,
                    file_missing: false,
                    tags: Vec::new(),
                })
                .collect(),
            accent_color: None,
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            created_at: SmolStr::new_static(""),
            updated_at: SmolStr::new_static(""),
            capsule_size: SmolStr::new_static("medium"),
            capsule_shape: SmolStr::new_static("pill"),
            locked: false,
            visible: true,
            stack_id: None,
            stack_order: 0,
            alias: None,
            display_mode: None,
            live_folder_path: None,
        }
    }

    fn make_snapshot(zones: Vec<BentoZone>) -> DesktopSnapshot {
        DesktopSnapshot {
            id: SmolStr::new_static("s"),
            name: "s".to_string(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
            zones,
            captured_at: SmolStr::new_static("2026-01-01T00:00:00Z"),
        }
    }

    #[test]
    fn delta_counts_zone_add() {
        let prev = make_snapshot(vec![make_zone("a", vec![])]);
        let cur = vec![make_zone("a", vec![]), make_zone("b", vec![])];
        let d = compute_delta(Some(&prev), &cur);
        assert_eq!(d.zones_added, 1);
        assert_eq!(d.zones_removed, 0);
    }

    #[test]
    fn delta_counts_item_add_remove() {
        let prev = make_snapshot(vec![make_zone("a", vec!["i1", "i2"])]);
        let cur = vec![make_zone("a", vec!["i2", "i3", "i4"])];
        let d = compute_delta(Some(&prev), &cur);
        assert_eq!(d.items_added, 2);
        assert_eq!(d.items_removed, 1);
        assert_eq!(d.items_moved, 0);
    }

    #[test]
    fn delta_detects_cross_zone_move() {
        let prev = make_snapshot(vec![make_zone("a", vec!["i1"]), make_zone("b", vec![])]);
        let cur = vec![make_zone("a", vec![]), make_zone("b", vec!["i1"])];
        let d = compute_delta(Some(&prev), &cur);
        assert_eq!(d.items_added, 0);
        assert_eq!(d.items_removed, 0);
        assert_eq!(d.items_moved, 1);
    }

    #[test]
    fn delta_detects_zone_geometry_update_without_timestamp_change() {
        let prev_zone = make_zone("a", vec![]);
        let mut current_zone = prev_zone.clone();
        current_zone.position.x_percent = 42.0;
        current_zone.updated_at = prev_zone.updated_at.clone();

        let prev = make_snapshot(vec![prev_zone]);
        let d = compute_delta(Some(&prev), &[current_zone]);

        assert_eq!(d.zones_updated, 1);
    }

    #[test]
    fn delta_ignores_timestamp_only_zone_change() {
        let prev_zone = make_zone("a", vec![]);
        let mut current_zone = prev_zone.clone();
        current_zone.updated_at = SmolStr::new_static("2026-01-02T00:00:00Z");

        let prev = make_snapshot(vec![prev_zone]);
        let d = compute_delta(Some(&prev), &[current_zone]);

        assert_eq!(d.zones_updated, 0);
    }

    #[test]
    fn delta_with_no_prev_treats_everything_as_added() {
        let cur = vec![make_zone("a", vec!["i1"]), make_zone("b", vec![])];
        let d = compute_delta(None, &cur);
        assert_eq!(d.zones_added, 2);
    }

    #[test]
    fn human_renders_empty_as_no_change() {
        assert_eq!(DeltaSummary::default().human(), "no change");
    }

    #[test]
    fn human_renders_mixed_summary() {
        let d = DeltaSummary {
            items_added: 3,
            zones_removed: 1,
            ..Default::default()
        };
        let s = d.human();
        assert!(s.contains("+3 items"));
        assert!(s.contains("-1 zones"));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let cp = Checkpoint {
            id: new_checkpoint_id(),
            snapshot: make_snapshot(vec![make_zone("a", vec![])]),
            delta: DeltaSummary::default(),
            delta_summary: "no change".to_string(),
            trigger: SmolStr::new_static("test"),
            coalesce_key: Some(SmolStr::new_static("roundtrip:test")),
            pinned: false,
        };
        store.save(&cp).expect("save");
        let loaded = store.load(&cp.id).expect("load");
        assert_eq!(loaded.id, cp.id);
        assert_eq!(loaded.trigger.as_str(), "test");
        assert_eq!(loaded.coalesce_key.as_deref(), Some("roundtrip:test"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_all_sorts_by_captured_at() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let mut s1 = make_snapshot(vec![]);
        s1.captured_at = SmolStr::new_static("2026-02-01T00:00:00Z");
        let mut s2 = make_snapshot(vec![]);
        s2.captured_at = SmolStr::new_static("2026-01-01T00:00:00Z");
        store
            .save(&Checkpoint {
                id: SmolStr::new_static("b-newer"),
                snapshot: s1,
                delta: DeltaSummary::default(),
                delta_summary: String::new(),
                trigger: SmolStr::new_static(""),
                coalesce_key: None,
                pinned: false,
            })
            .expect("save b");
        store
            .save(&Checkpoint {
                id: SmolStr::new_static("a-older"),
                snapshot: s2,
                delta: DeltaSummary::default(),
                delta_summary: String::new(),
                trigger: SmolStr::new_static(""),
                coalesce_key: None,
                pinned: false,
            })
            .expect("save a");
        let all = store.load_all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id.as_str(), "a-older");
        assert_eq!(all[1].id.as_str(), "b-newer");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_existing_checkpoint() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let cp = Checkpoint {
            id: SmolStr::new_static("to-del"),
            snapshot: make_snapshot(vec![]),
            delta: DeltaSummary::default(),
            delta_summary: String::new(),
            trigger: SmolStr::new_static(""),
            coalesce_key: None,
            pinned: false,
        };
        store.save(&cp).expect("save");
        assert!(store.load("to-del").is_ok());
        store.delete("to-del").expect("delete");
        assert!(store.load("to-del").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_nonexistent_is_idempotent() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        store.delete("missing").expect("idempotent");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_checkpoint_id_is_lexicographically_monotonic() {
        let id1 = new_checkpoint_id();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = new_checkpoint_id();
        assert!(id2.as_str() > id1.as_str(), "{id2} should sort after {id1}");
    }

    #[test]
    fn meta_round_trip_preserves_zone_and_item_counts() {
        let cp = Checkpoint {
            id: SmolStr::new_static("c"),
            snapshot: make_snapshot(vec![
                make_zone("a", vec!["i1", "i2"]),
                make_zone("b", vec!["i3"]),
            ]),
            delta: DeltaSummary::default(),
            delta_summary: String::new(),
            trigger: SmolStr::new_static(""),
            coalesce_key: None,
            pinned: false,
        };
        let meta = CheckpointMeta::from(&cp);
        assert_eq!(meta.zone_count, 2);
        assert_eq!(meta.item_count, 3);
    }
}
