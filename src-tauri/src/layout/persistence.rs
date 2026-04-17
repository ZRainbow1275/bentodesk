use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;

use crate::error::BentoDeskError;
use crate::storage;

/// Represents a zone's position as percentage of screen dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelativePosition {
    pub x_percent: f64,
    pub y_percent: f64,
}

/// Represents a zone's expanded size as percentage of screen dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelativeSize {
    pub w_percent: f64,
    pub h_percent: f64,
}

/// Position within the item grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridPosition {
    pub col: u32,
    pub row: u32,
    pub col_span: u32,
}

/// Type of desktop item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemType {
    File,
    Folder,
    Shortcut,
    Application,
}

/// Automatic grouping rule type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GroupRuleType {
    Extension,
    ModifiedDate,
    NamePattern,
}

/// Configuration for automatic file grouping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoGroupRule {
    pub rule_type: GroupRuleType,
    pub pattern: Option<String>,
    pub extensions: Option<Vec<String>>,
}

/// A single desktop item within a zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BentoItem {
    pub id: String,
    pub zone_id: String,
    pub item_type: ItemType,
    pub name: String,
    pub path: String,
    pub icon_hash: String,
    pub grid_position: GridPosition,
    pub is_wide: bool,
    pub added_at: String,
    /// Original file path on the Desktop. The file is moved from here into
    /// `.bentodesk/` when hidden. `None` if the item was not hidden (e.g. non-Desktop source).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
    /// Current file path inside the `.bentodesk/` hidden subfolder.
    /// Used by `restore_file` to move the file back to `original_path`.
    /// `None` if the item was not hidden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden_path: Option<String>,
    /// Whether the referenced file is missing (deleted externally).
    /// Zone item is preserved but marked as missing so the UI can indicate it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub file_missing: bool,
    /// Desktop icon X coordinate at the time the item was hidden.
    /// Used to restore the icon to its original position when removed from a zone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_x: Option<i32>,
    /// Desktop icon Y coordinate at the time the item was hidden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_y: Option<i32>,
}

/// A Bento zone containing organized desktop items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BentoZone {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub position: RelativePosition,
    pub expanded_size: RelativeSize,
    pub items: Vec<BentoItem>,
    pub accent_color: Option<String>,
    pub sort_order: i32,
    pub auto_group: Option<AutoGroupRule>,
    pub grid_columns: u32,
    pub created_at: String,
    pub updated_at: String,
    /// Capsule size variant: "small", "medium", or "large". Defaults to "medium".
    #[serde(default = "default_capsule_size")]
    pub capsule_size: String,
    /// Capsule shape variant: "pill", "rounded", "circle", or "minimal". Defaults to "pill".
    #[serde(default = "default_capsule_shape")]
    pub capsule_shape: String,
}

fn default_capsule_size() -> String {
    "medium".to_string()
}

fn default_capsule_shape() -> String {
    "pill".to_string()
}

/// Partial update for a zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneUpdate {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub position: Option<RelativePosition>,
    pub expanded_size: Option<RelativeSize>,
    pub accent_color: Option<String>,
    pub grid_columns: Option<u32>,
    pub auto_group: Option<AutoGroupRule>,
    pub capsule_size: Option<String>,
    pub capsule_shape: Option<String>,
}

/// Top-level layout data persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutData {
    pub version: String,
    pub zones: Vec<BentoZone>,
    pub last_modified: String,
    /// Optional coherence token used by recovery bundles to verify that a
    /// snapshot matches the layout that produced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coherence_id: Option<String>,
}

impl LayoutData {
    /// Load layout from disk, or return a default empty layout.
    ///
    /// Uses [`storage::read_json_with_recovery`] so that a corrupt primary file
    /// is automatically healed from the `.bak` sibling created by prior saves.
    pub fn load_or_default(handle: &AppHandle) -> Result<Self, BentoDeskError> {
        let path = Self::layout_path(handle);
        match storage::read_json_with_recovery::<LayoutData>(&path, "Layout") {
            Ok(Some(data)) => Ok(data),
            Ok(None) => Ok(Self::default()),
            Err(e) => {
                tracing::error!(
                    "Layout load failed even after backup recovery, using default: {e}"
                );
                Ok(Self::default())
            }
        }
    }

    /// Atomically persist layout data to disk.
    ///
    /// Writes to a temporary file, flushes, then swaps into place via
    /// [`storage::write_json_atomic`]. The previous primary file is retained as
    /// a `.bak` sibling for crash recovery.
    pub fn save(&self, handle: &AppHandle) -> Result<(), BentoDeskError> {
        let path = Self::layout_path(handle);
        storage::write_json_atomic(&path, self)
    }

    /// Resolve the path where layout.json lives.
    fn layout_path(handle: &AppHandle) -> PathBuf {
        let base = Self::data_dir(handle);
        base.join("layout.json")
    }

    /// Determine the data directory (portable or AppData).
    fn data_dir(handle: &AppHandle) -> PathBuf {
        // Check portable mode: if a `data` directory exists next to the executable
        if let Ok(exe_path) = std::env::current_exe() {
            let portable_dir = exe_path.parent().map(|p| p.join("data"));
            if let Some(ref dir) = portable_dir {
                if dir.exists() {
                    return dir.clone();
                }
            }
        }
        // Fall back to AppData
        tauri::Manager::path(handle)
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

impl Default for LayoutData {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_string(),
            zones: Vec::new(),
            last_modified: chrono::Utc::now().to_rfc3339(),
            coherence_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_data_default_has_no_zones() {
        let layout = LayoutData::default();
        assert!(layout.zones.is_empty());
        assert_eq!(layout.version, "1.0.0");
        assert!(!layout.last_modified.is_empty());
    }

    #[test]
    fn layout_data_serialization_roundtrip() {
        let layout = LayoutData {
            version: "1.0.0".to_string(),
            zones: vec![BentoZone {
                id: "zone-1".to_string(),
                name: "Documents".to_string(),
                icon: "D".to_string(),
                position: RelativePosition {
                    x_percent: 10.0,
                    y_percent: 20.0,
                },
                expanded_size: RelativeSize {
                    w_percent: 30.0,
                    h_percent: 40.0,
                },
                items: vec![BentoItem {
                    id: "item-1".to_string(),
                    zone_id: "zone-1".to_string(),
                    item_type: ItemType::File,
                    name: "readme.txt".to_string(),
                    path: "C:\\Desktop\\readme.txt".to_string(),
                    icon_hash: "abc123".to_string(),
                    grid_position: GridPosition {
                        col: 0,
                        row: 0,
                        col_span: 1,
                    },
                    is_wide: false,
                    added_at: "2026-01-01T00:00:00Z".to_string(),
                    original_path: None,
                    hidden_path: None,
                    icon_x: None,
                    icon_y: None,
                    file_missing: false,
                }],
                accent_color: Some("#ff0000".to_string()),
                sort_order: 0,
                auto_group: None,
                grid_columns: 4,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                capsule_size: "medium".to_string(),
                capsule_shape: "pill".to_string(),
            }],
            last_modified: "2026-01-01T00:00:00Z".to_string(),
            coherence_id: None,
        };

        let json = serde_json::to_string(&layout).unwrap();
        let deserialized: LayoutData = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.zones.len(), 1);
        assert_eq!(deserialized.zones[0].id, "zone-1");
        assert_eq!(deserialized.zones[0].items.len(), 1);
        assert_eq!(deserialized.zones[0].items[0].name, "readme.txt");
        assert_eq!(
            deserialized.zones[0].accent_color,
            Some("#ff0000".to_string())
        );
    }

    #[test]
    fn zone_update_partial_fields() {
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
        };
        let json = serde_json::to_string(&update).unwrap();
        let parsed: ZoneUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, Some("New Name".to_string()));
        assert_eq!(parsed.grid_columns, Some(6));
        assert!(parsed.icon.is_none());
    }

    #[test]
    fn item_type_serialization() {
        let item_type = ItemType::Application;
        let json = serde_json::to_string(&item_type).unwrap();
        assert_eq!(json, "\"Application\"");

        let parsed: ItemType = serde_json::from_str("\"Shortcut\"").unwrap();
        assert!(matches!(parsed, ItemType::Shortcut));
    }

    #[test]
    fn auto_group_rule_serialization() {
        let rule = AutoGroupRule {
            rule_type: GroupRuleType::Extension,
            pattern: None,
            extensions: Some(vec!["pdf".to_string(), "doc".to_string()]),
        };
        let json = serde_json::to_string(&rule).unwrap();
        let parsed: AutoGroupRule = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.rule_type, GroupRuleType::Extension));
        assert_eq!(parsed.extensions.unwrap().len(), 2);
    }

    #[test]
    fn corrupt_json_returns_serde_error() {
        let result = serde_json::from_str::<LayoutData>("{ not valid json }");
        assert!(result.is_err());
    }
}
