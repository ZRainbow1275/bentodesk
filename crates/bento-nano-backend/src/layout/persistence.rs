//! T-097 — layout persistence types + load/save surface.
//!
//! The 24+ fields on [`BentoZone`] preserve the 1.x v1.2 schema verbatim so
//! existing user `layout.json` files round-trip without migration. Field
//! comments preserve the 1.x rationale.
//!
//! `String` is used for filesystem-path fields (`path`, `live_folder_path`,
//! `original_path`, `hidden_path`) because Windows paths routinely exceed
//! the `SmolStr` 22-byte inline budget. Short identifiers (`id`, `name`,
//! `icon`, `accent_color`, `capsule_size`, `capsule_shape`, `display_mode`,
//! `version`, the timestamp fields) use [`SmolStr`] per spec §10.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
use smol_str::SmolStr;

use crate::time;

/// Distinguish `field: null` (explicit clear, `Some(None)`) from `field`
/// absent (unchanged, outer `None`) when deserialising
/// `Option<Option<T>>` fields on `ZoneUpdate`. Without this, both shapes
/// collapse to outer `None` and the "clear this field" semantic is lost.
fn deserialize_double_option<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(d).map(Some)
}

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the layout persistence module.
#[derive(Debug)]
pub enum LayoutError {
    /// `std::fs::read` / `std::fs::write` failed for the layout file.
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

impl core::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "layout I/O failed for {}: {source}", path.display())
            }
            Self::Serde { path, source } => {
                write!(f, "layout JSON failed for {}: {source}", path.display())
            }
        }
    }
}

impl core::error::Error for LayoutError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Serde { source, .. } => Some(source),
        }
    }
}

// ─── Value types ─────────────────────────────────────────────────────

/// A zone's position as percentage of screen dimensions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RelativePosition {
    pub x_percent: f64,
    pub y_percent: f64,
}

/// A zone's expanded size as percentage of screen dimensions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RelativeSize {
    pub w_percent: f64,
    pub h_percent: f64,
}

/// Position within the item grid.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct GridPosition {
    pub col: u32,
    pub row: u32,
    pub col_span: u32,
}

/// Type of desktop item.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ItemType {
    File,
    Folder,
    Shortcut,
    Application,
}

/// Automatic grouping rule type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupRuleType {
    Extension,
    ModifiedDate,
    NamePattern,
}

/// Configuration for automatic file grouping. Same shape as 1.x.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoGroupRule {
    pub rule_type: GroupRuleType,
    pub pattern: Option<String>,
    pub extensions: Option<Vec<String>>,
}

// ─── BentoItem ───────────────────────────────────────────────────────

/// A single desktop item within a zone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BentoItem {
    pub id: SmolStr,
    pub zone_id: SmolStr,
    pub item_type: ItemType,
    pub name: String,
    pub path: String,
    pub icon_hash: SmolStr,
    pub grid_position: GridPosition,
    pub is_wide: bool,
    pub added_at: SmolStr,

    /// Original file path on the Desktop. The file is moved from here into
    /// `.bentodesk/` when hidden. `None` if the item was not hidden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,

    /// Current file path inside the `.bentodesk/` hidden subfolder. Used by
    /// `restore_file` to move the file back to `original_path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden_path: Option<String>,

    /// Whether the referenced file is missing (deleted externally).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub file_missing: bool,

    /// Desktop icon X coordinate at the time the item was hidden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_x: Option<i32>,

    /// Desktop icon Y coordinate at the time the item was hidden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_y: Option<i32>,

    /// User/rules metadata tags attached to this item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<SmolStr>,
}

// ─── BentoZone (24+ fields, master plan §11 Q5) ──────────────────────

/// A Bento zone containing organized desktop items.
///
/// All v1.2 fields preserved — `stack_id`, `stack_order`, `alias`,
/// `display_mode`, `live_folder_path` round-trip across versions. `Default`
/// is **not** implemented — there is no meaningful zero zone; callers must
/// fill required fields explicitly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BentoZone {
    pub id: SmolStr,
    pub name: String,
    pub icon: SmolStr,
    pub position: RelativePosition,
    pub expanded_size: RelativeSize,
    pub items: Vec<BentoItem>,
    pub accent_color: Option<SmolStr>,
    pub sort_order: i32,
    pub auto_group: Option<AutoGroupRule>,
    pub grid_columns: u32,
    pub created_at: SmolStr,
    pub updated_at: SmolStr,

    /// Capsule size variant — `"small"` / `"medium"` / `"large"`. Defaults to
    /// `"medium"`.
    #[serde(default = "default_capsule_size")]
    pub capsule_size: SmolStr,

    /// Capsule shape variant — `"pill"` / `"rounded"` / `"circle"` /
    /// `"minimal"`. Defaults to `"pill"`.
    #[serde(default = "default_capsule_shape")]
    pub capsule_shape: SmolStr,

    /// When true, the zone is read-only for layout gestures.
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub locked: bool,

    /// When false, the selected-stack canvas keeps the zone persisted but
    /// omits it from render and hit-test paths.
    #[serde(default = "default_true_bool", skip_serializing_if = "is_true_bool")]
    pub visible: bool,

    /// D2: stack identifier — zones sharing the same `stack_id` form a visual
    /// stack. `None` = free-standing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<SmolStr>,

    /// D2: position within the owning stack (0 = bottom, N-1 = top).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub stack_order: u32,

    /// D3: user-defined display alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Optional per-zone override for reveal behaviour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_mode: Option<SmolStr>,

    /// E2-e: when set, the zone's items list is a live read-only mirror of
    /// this folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_folder_path: Option<String>,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}
fn is_false_bool(v: &bool) -> bool {
    !*v
}
fn is_true_bool(v: &bool) -> bool {
    *v
}
fn default_true_bool() -> bool {
    true
}
fn default_capsule_size() -> SmolStr {
    SmolStr::new_static("medium")
}
fn default_capsule_shape() -> SmolStr {
    SmolStr::new_static("pill")
}

// ─── ZoneUpdate (partial mutation) ───────────────────────────────────

/// Partial update for a zone. `Some(Some("..."))` sets, `Some(None)` clears,
/// `None` leaves unchanged for the `Option<Option<…>>` fields (1.x behaviour).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneUpdate {
    pub name: Option<String>,
    pub icon: Option<SmolStr>,
    pub position: Option<RelativePosition>,
    pub expanded_size: Option<RelativeSize>,
    pub accent_color: Option<SmolStr>,
    pub grid_columns: Option<u32>,
    pub auto_group: Option<AutoGroupRule>,
    pub capsule_size: Option<SmolStr>,
    pub capsule_shape: Option<SmolStr>,
    pub locked: Option<bool>,

    /// D3: `Some(Some("…"))` sets alias, `Some(None)` clears, `None` unchanged.
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub alias: Option<Option<String>>,

    /// `Some(Some("hover"))` sets mode, `Some(None)` clears to inherit global
    /// settings.
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub display_mode: Option<Option<SmolStr>>,
}

// ─── LayoutData ──────────────────────────────────────────────────────

/// Top-level layout data persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayoutData {
    pub version: SmolStr,
    pub zones: Vec<BentoZone>,
    pub last_modified: SmolStr,

    /// Optional coherence token used by recovery bundles to verify that a
    /// snapshot matches the layout that produced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coherence_id: Option<SmolStr>,
}

impl LayoutData {
    /// Load layout from `path`. Returns [`Self::default`] when the file does
    /// not exist (first-launch case). Returns `Err` on hard I/O or parse
    /// failure — caller decides whether to fall back.
    ///
    /// **Note**: this is a plain-`std::fs` read with no `.bak` recovery. Atomic
    /// write + crash recovery is the dispatcher's job (calls `crate::storage`
    /// once that module stabilises).
    pub fn load(path: &Path) -> Result<Self, LayoutError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(path).map_err(|e| LayoutError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        serde_json::from_slice(&bytes).map_err(|e| LayoutError::Serde {
            path: path.to_path_buf(),
            source: e,
        })
    }

    /// Persist the layout to `path`. Plain-`std::fs::write`; atomic swap +
    /// `.bak` is the dispatcher's job.
    pub fn save(&self, path: &Path) -> Result<(), LayoutError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| LayoutError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| LayoutError::Serde {
            path: path.to_path_buf(),
            source: e,
        })?;
        std::fs::write(path, &bytes).map_err(|e| LayoutError::Io {
            path: path.to_path_buf(),
            source: e,
        })
    }

    /// Refresh `last_modified` to "now" using [`crate::time::now_rfc3339`].
    /// Call this every time the layout mutates so downstream consumers
    /// (timeline, recovery_bundle) can detect changes by string compare.
    pub fn touch(&mut self) {
        self.last_modified = SmolStr::from(time::now_rfc3339());
    }
}

impl Default for LayoutData {
    fn default() -> Self {
        Self {
            version: SmolStr::new_static("1.0.0"),
            zones: Vec::new(),
            last_modified: SmolStr::from(time::now_rfc3339()),
            coherence_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_zone(id: &str) -> BentoZone {
        BentoZone {
            id: SmolStr::from(id),
            name: "Test".to_string(),
            icon: SmolStr::new_static("T"),
            position: RelativePosition {
                x_percent: 10.0,
                y_percent: 20.0,
            },
            expanded_size: RelativeSize {
                w_percent: 30.0,
                h_percent: 40.0,
            },
            items: Vec::new(),
            accent_color: None,
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            created_at: SmolStr::new_static("2026-01-01T00:00:00Z"),
            updated_at: SmolStr::new_static("2026-01-01T00:00:00Z"),
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

    #[test]
    fn layout_data_default_has_no_zones() {
        let layout = LayoutData::default();
        assert!(layout.zones.is_empty());
        assert_eq!(layout.version.as_str(), "1.0.0");
        assert!(!layout.last_modified.is_empty());
    }

    #[test]
    fn layout_data_serialization_roundtrip() {
        let layout = LayoutData {
            version: SmolStr::new_static("1.0.0"),
            zones: vec![make_zone("zone-1")],
            last_modified: SmolStr::new_static("2026-01-01T00:00:00Z"),
            coherence_id: None,
        };
        let json = serde_json::to_string(&layout).expect("serialize");
        let parsed: LayoutData = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, layout);
    }

    #[test]
    fn touch_updates_last_modified() {
        let mut layout = LayoutData::default();
        let before = layout.last_modified.clone();
        std::thread::sleep(std::time::Duration::from_millis(2));
        layout.touch();
        assert_ne!(layout.last_modified, before);
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let path = PathBuf::from("Z:/does/not/exist/layout.json");
        let layout = LayoutData::load(&path).expect("nonexistent → default");
        assert!(layout.zones.is_empty());
    }

    #[test]
    fn item_type_serialization() {
        let json = serde_json::to_string(&ItemType::Application).expect("ser");
        assert_eq!(json, "\"Application\"");
        let parsed: ItemType = serde_json::from_str("\"Shortcut\"").expect("de");
        assert!(matches!(parsed, ItemType::Shortcut));
    }

    #[test]
    fn auto_group_rule_round_trip() {
        let rule = AutoGroupRule {
            rule_type: GroupRuleType::Extension,
            pattern: None,
            extensions: Some(vec!["pdf".to_string(), "doc".to_string()]),
        };
        let json = serde_json::to_string(&rule).expect("ser");
        let parsed: AutoGroupRule = serde_json::from_str(&json).expect("de");
        assert_eq!(parsed.rule_type, GroupRuleType::Extension);
        assert_eq!(parsed.extensions.as_ref().map(|v| v.len()), Some(2));
    }

    #[test]
    fn skip_serializing_if_strips_default_optionals() {
        let zone = make_zone("z-min");
        let json = serde_json::to_string(&zone).expect("ser");
        assert!(!json.contains("\"stack_id\""));
        assert!(!json.contains("\"stack_order\""));
        assert!(!json.contains("\"alias\""));
        assert!(!json.contains("\"display_mode\""));
        assert!(!json.contains("\"live_folder_path\""));
        assert!(!json.contains("\"locked\""));
        assert!(!json.contains("\"visible\""));
    }

    #[test]
    fn zone_visibility_defaults_true_and_hidden_serializes() {
        let legacy = r#"{
            "id":"z-legacy",
            "name":"Legacy",
            "icon":"T",
            "position":{"x_percent":10.0,"y_percent":20.0},
            "expanded_size":{"w_percent":30.0,"h_percent":40.0},
            "items":[],
            "accent_color":null,
            "sort_order":0,
            "auto_group":null,
            "grid_columns":4,
            "created_at":"2026-01-01T00:00:00Z",
            "updated_at":"2026-01-01T00:00:00Z"
        }"#;
        let parsed: BentoZone = serde_json::from_str(legacy).expect("legacy zone");
        assert!(parsed.visible);

        let mut hidden = parsed.clone();
        hidden.visible = false;
        let json = serde_json::to_string(&hidden).expect("ser");
        assert!(json.contains("\"visible\":false"));
    }

    #[test]
    fn zone_update_partial_fields_round_trip() {
        let update = ZoneUpdate {
            name: Some("New Name".to_string()),
            icon: None,
            position: None,
            expanded_size: None,
            accent_color: None,
            grid_columns: Some(6),
            auto_group: None,
            capsule_size: None,
            capsule_shape: None,
            locked: None,
            alias: Some(None),
            display_mode: Some(Some(SmolStr::new_static("hover"))),
        };
        let json = serde_json::to_string(&update).expect("ser");
        let parsed: ZoneUpdate = serde_json::from_str(&json).expect("de");
        assert_eq!(parsed.name.as_deref(), Some("New Name"));
        assert_eq!(parsed.grid_columns, Some(6));
        assert!(parsed.icon.is_none());
        assert!(matches!(parsed.alias, Some(None)));
    }

    #[test]
    fn corrupt_json_returns_serde_error() {
        let path = std::env::temp_dir().join("bento-nano-layout-corrupt-test.json");
        std::fs::write(&path, b"{ not valid json }").expect("write fixture");
        let err = LayoutData::load(&path).expect_err("must fail");
        let _ = std::fs::remove_file(&path);
        assert!(matches!(err, LayoutError::Serde { .. }));
    }
}
