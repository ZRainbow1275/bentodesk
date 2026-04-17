//! Checkpoint model + on-disk store for the time-machine timeline.
//!
//! A [`Checkpoint`] reuses the existing [`DesktopSnapshot`] to record the full
//! zone set at a point in time, and augments it with a human-readable delta
//! summary and the trigger that caused the capture. Storage is a flat directory
//! of `checkpoint-{timestamp}.json` files under `%APPDATA%/BentoDesk/timeline/`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::BentoDeskError;
use crate::layout::snapshot::DesktopSnapshot;

/// A structured diff description between two checkpoints, used both for the
/// slider tooltip (`"+3 items, -1 zone"`) and for the `on_significant_change`
/// threshold decision.
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

/// The on-disk checkpoint record.
///
/// `id` is the file-safe timestamp (e.g. `20260417T120501Z-<uuid8>`) also used
/// as the directory entry stem, so listing can sort lexicographically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub snapshot: DesktopSnapshot,
    #[serde(default)]
    pub delta: DeltaSummary,
    #[serde(default)]
    pub delta_summary: String,
    #[serde(default)]
    pub trigger: String,
    /// Manual / pinned checkpoints are never evicted by the ring buffer.
    #[serde(default)]
    pub pinned: bool,
}

/// Lightweight metadata sent to the frontend — avoids shipping full zone data
/// in the initial `list_checkpoints` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMeta {
    pub id: String,
    pub captured_at: String,
    pub trigger: String,
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

/// Disk-backed checkpoint store. All I/O is best-effort — failures are logged
/// but never propagated, so a broken timeline never blocks a user mutation.
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

    pub fn save(&self, cp: &Checkpoint) -> Result<(), BentoDeskError> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("checkpoint-{}.json", cp.id));
        let content = serde_json::to_string(cp)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Checkpoint, BentoDeskError> {
        let path = self.dir.join(format!("checkpoint-{id}.json"));
        let content = std::fs::read_to_string(path)?;
        let cp: Checkpoint = serde_json::from_str(&content)?;
        Ok(cp)
    }

    pub fn delete(&self, id: &str) -> Result<(), BentoDeskError> {
        let path = self.dir.join(format!("checkpoint-{id}.json"));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Load every checkpoint. Invalid files are skipped with a warning.
    pub fn load_all(&self) -> Vec<Checkpoint> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(_) => return out,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                match std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| serde_json::from_str::<Checkpoint>(&s).ok())
                {
                    Some(cp) => out.push(cp),
                    None => {
                        tracing::warn!("Timeline: skipping unreadable checkpoint file {:?}", path);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.snapshot.captured_at.cmp(&b.snapshot.captured_at));
        out
    }
}

/// Generate a file-safe, lexicographically-sortable checkpoint id.
pub fn new_checkpoint_id() -> String {
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ");
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];
    format!("{ts}-{suffix}")
}

/// Compute the delta between a previous snapshot and the current zones.
pub fn compute_delta(
    prev: Option<&DesktopSnapshot>,
    current_zones: &[crate::layout::persistence::BentoZone],
) -> DeltaSummary {
    let mut d = DeltaSummary::default();
    let prev_zones = prev.map(|s| s.zones.as_slice()).unwrap_or(&[]);

    let prev_ids: std::collections::HashSet<&str> =
        prev_zones.iter().map(|z| z.id.as_str()).collect();
    let cur_ids: std::collections::HashSet<&str> =
        current_zones.iter().map(|z| z.id.as_str()).collect();

    d.zones_added = cur_ids.difference(&prev_ids).count() as i32;
    d.zones_removed = prev_ids.difference(&cur_ids).count() as i32;

    // Global item id sets across all zones — gives us true add/remove counts
    // without double-counting moves between zones.
    let prev_item_ids: std::collections::HashSet<&str> = prev_zones
        .iter()
        .flat_map(|z| z.items.iter().map(|i| i.id.as_str()))
        .collect();
    let cur_item_ids: std::collections::HashSet<&str> = current_zones
        .iter()
        .flat_map(|z| z.items.iter().map(|i| i.id.as_str()))
        .collect();
    d.items_added = cur_item_ids.difference(&prev_item_ids).count() as i32;
    d.items_removed = prev_item_ids.difference(&cur_item_ids).count() as i32;

    // Track zone membership for items that exist in both snapshots — any
    // difference is a move.
    let prev_item_zone: std::collections::HashMap<&str, &str> = prev_zones
        .iter()
        .flat_map(|z| z.items.iter().map(move |i| (i.id.as_str(), z.id.as_str())))
        .collect();
    for cur in current_zones {
        for item in &cur.items {
            if let Some(&prev_zone_id) = prev_item_zone.get(item.id.as_str()) {
                if prev_zone_id != cur.id.as_str() {
                    d.items_moved += 1;
                }
            }
        }
        if let Some(prev) = prev_zones.iter().find(|z| z.id == cur.id) {
            if cur.updated_at != prev.updated_at {
                d.zones_updated += 1;
            }
        }
    }

    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::persistence::{
        BentoItem, BentoZone, GridPosition, ItemType, RelativePosition, RelativeSize,
    };
    use crate::layout::resolution::Resolution;

    fn make_zone(id: &str, items: Vec<&str>) -> BentoZone {
        BentoZone {
            id: id.to_string(),
            name: format!("Z-{id}"),
            icon: "folder".to_string(),
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
                    id: iid.to_string(),
                    zone_id: id.to_string(),
                    item_type: ItemType::File,
                    name: iid.to_string(),
                    path: format!("C:/{iid}"),
                    icon_hash: String::new(),
                    grid_position: GridPosition {
                        col: 0,
                        row: 0,
                        col_span: 1,
                    },
                    is_wide: false,
                    added_at: String::new(),
                    original_path: None,
                    hidden_path: None,
                    icon_x: None,
                    icon_y: None,
                    file_missing: false,
                })
                .collect(),
            accent_color: None,
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            created_at: String::new(),
            updated_at: String::new(),
            capsule_size: "medium".to_string(),
            capsule_shape: "pill".to_string(),
        }
    }

    fn make_snapshot(zones: Vec<BentoZone>) -> DesktopSnapshot {
        DesktopSnapshot {
            id: "s".to_string(),
            name: "s".to_string(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
            zones,
            captured_at: "2026-01-01T00:00:00Z".to_string(),
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
        // i1 migrated from zone a to zone b; no adds/removes.
        let cur = vec![make_zone("a", vec![]), make_zone("b", vec!["i1"])];
        let d = compute_delta(Some(&prev), &cur);
        assert_eq!(d.items_added, 0);
        assert_eq!(d.items_removed, 0);
        assert_eq!(d.items_moved, 1);
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
            items_removed: 0,
            items_moved: 0,
            zones_added: 0,
            zones_removed: 1,
            zones_updated: 0,
        };
        let s = d.human();
        assert!(s.contains("+3 items"));
        assert!(s.contains("-1 zones"));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().to_path_buf());
        let cp = Checkpoint {
            id: new_checkpoint_id(),
            snapshot: make_snapshot(vec![make_zone("a", vec![])]),
            delta: DeltaSummary::default(),
            delta_summary: "no change".to_string(),
            trigger: "test".to_string(),
            pinned: false,
        };
        store.save(&cp).unwrap();
        let loaded = store.load(&cp.id).unwrap();
        assert_eq!(loaded.id, cp.id);
        assert_eq!(loaded.trigger, "test");
    }

    #[test]
    fn load_all_sorts_by_captured_at() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().to_path_buf());
        let mut s1 = make_snapshot(vec![]);
        s1.captured_at = "2026-02-01T00:00:00Z".to_string();
        let mut s2 = make_snapshot(vec![]);
        s2.captured_at = "2026-01-01T00:00:00Z".to_string();
        store
            .save(&Checkpoint {
                id: "b-newer".to_string(),
                snapshot: s1,
                delta: DeltaSummary::default(),
                delta_summary: String::new(),
                trigger: String::new(),
                pinned: false,
            })
            .unwrap();
        store
            .save(&Checkpoint {
                id: "a-older".to_string(),
                snapshot: s2,
                delta: DeltaSummary::default(),
                delta_summary: String::new(),
                trigger: String::new(),
                pinned: false,
            })
            .unwrap();
        let all = store.load_all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "a-older");
        assert_eq!(all[1].id, "b-newer");
    }
}
