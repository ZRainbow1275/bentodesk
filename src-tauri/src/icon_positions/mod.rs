//! Desktop icon position save/restore.
//!
//! BentoDesk intercepts the desktop icon grid. When the application starts,
//! it saves the current icon layout to a backup file. When the application
//! exits, it restores the original positions so the user's desktop is
//! returned to its pre-BentoDesk state.
//!
//! # Architecture
//! - [`finder`] — Locates the desktop `IFolderView` via COM.
//! - [`reader`] — Reads icon display names and positions.
//! - [`writer`] — Restores icon positions via `SelectAndPositionItems`.
//!
//! # Usage
//! ```rust,ignore
//! // On startup:
//! let layout = icon_positions::save_layout()?;
//! // On exit:
//! icon_positions::restore_layout(&layout)?;
//! ```

pub(crate) mod finder;
pub(crate) mod reader;
pub(crate) mod writer;

use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::AppHandle;

use crate::error::BentoDeskError;
use crate::layout::resolution;

/// A single desktop icon's display name and pixel position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconPosition {
    /// Icon display name (e.g. "Recycle Bin", "document.pdf").
    pub name: String,
    /// Horizontal pixel coordinate in desktop logical coordinates.
    pub x: i32,
    /// Vertical pixel coordinate in desktop logical coordinates.
    pub y: i32,
}

/// Complete snapshot of all desktop icon positions at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedIconLayout {
    /// All icon positions captured in this snapshot.
    pub icons: Vec<IconPosition>,
    /// ISO 8601 timestamp when this snapshot was taken.
    pub saved_at: String,
    /// Screen resolution at the time of capture.
    pub resolution: resolution::Resolution,
    /// DPI scale factor at the time of capture (e.g. 1.0 = 96 DPI).
    pub dpi: f64,
}

/// The default backup filename stored in the BentoDesk data directory.
const BACKUP_FILENAME: &str = "icon_layout_backup.json";

/// Save the current desktop icon layout.
///
/// Acquires the desktop `IFolderView` via COM, enumerates all icons,
/// and returns a [`SavedIconLayout`] with their names and positions.
///
/// This function initializes COM (STA) for the duration of the call.
pub fn save_layout() -> Result<SavedIconLayout, BentoDeskError> {
    tracing::info!("Saving desktop icon positions...");

    let (_guard, folder_view) = finder::find_desktop_folder_view()?;
    let icons = reader::read_all_icon_positions(&folder_view)?;

    let layout = SavedIconLayout {
        icons,
        saved_at: chrono::Utc::now().to_rfc3339(),
        resolution: resolution::get_current_resolution(),
        dpi: resolution::get_dpi_scale(),
    };

    tracing::info!(
        "Saved {} icon positions ({}x{} @ {:.2}x DPI)",
        layout.icons.len(),
        layout.resolution.width,
        layout.resolution.height,
        layout.dpi,
    );

    Ok(layout)
}

/// Restore desktop icon positions from a previously saved layout.
///
/// Acquires the desktop `IFolderView` via COM, matches saved icons to
/// current desktop icons by display name, and repositions them.
///
/// Returns `Ok(())` on success. Icons that no longer exist on the desktop
/// are silently skipped. If auto-arrange is enabled, it is temporarily
/// disabled for the duration of the restore.
pub fn restore_layout(saved: &SavedIconLayout) -> Result<(), BentoDeskError> {
    if saved.icons.is_empty() {
        tracing::warn!("No icon positions to restore (empty backup)");
        return Ok(());
    }

    tracing::info!(
        "Restoring {} icon positions from backup (saved at {})",
        saved.icons.len(),
        saved.saved_at,
    );

    let (_guard, folder_view) = finder::find_desktop_folder_view()?;
    let result = writer::restore_icon_positions(&folder_view, saved)?;

    if result.failed > 0 {
        tracing::warn!(
            "Restore completed with {} failures ({} restored, {} skipped)",
            result.failed,
            result.restored,
            result.skipped,
        );
    }

    Ok(())
}

/// Save the current icon layout to a JSON file in the given data directory.
///
/// The file is written to `{data_dir}/icon_layout_backup.json`.
pub fn persist_to_file(
    layout: &SavedIconLayout,
    data_dir: &std::path::Path,
) -> Result<(), BentoDeskError> {
    std::fs::create_dir_all(data_dir)?;
    let path = data_dir.join(BACKUP_FILENAME);
    let json = serde_json::to_string_pretty(layout)?;
    std::fs::write(&path, &json)?;
    tracing::debug!("Icon layout backup written to {}", path.display());
    Ok(())
}

/// Load a previously saved icon layout from JSON in the given data directory.
///
/// Returns `Ok(None)` if the backup file does not exist.
pub fn load_from_file(
    data_dir: &std::path::Path,
) -> Result<Option<SavedIconLayout>, BentoDeskError> {
    let path = data_dir.join(BACKUP_FILENAME);
    if !path.exists() {
        tracing::debug!("No icon layout backup found at {}", path.display());
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path)?;
    let layout: SavedIconLayout = serde_json::from_str(&json)?;
    tracing::debug!(
        "Loaded icon layout backup with {} icons from {}",
        layout.icons.len(),
        path.display(),
    );
    Ok(Some(layout))
}

/// Look up the saved position for a desktop icon by its display name.
///
/// Returns `Some((x, y))` if the icon was found in the saved layout.
pub fn lookup_icon_position(saved: &SavedIconLayout, name: &str) -> Option<(i32, i32)> {
    saved
        .icons
        .iter()
        .find(|i| i.name == name)
        .map(|i| (i.x, i.y))
}

/// Derive the display name Windows shows on the desktop for a filesystem path.
///
/// Shortcut-like items strip `.lnk` / `.url` from the visible caption.
pub fn display_name_from_path(path: &Path) -> Option<String> {
    let extension = path.extension().and_then(|ext| ext.to_str());
    if extension
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk") || ext.eq_ignore_ascii_case("url"))
    {
        path.file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
    } else {
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
    }
}

/// Look up a desktop icon position using the most stable path-derived aliases.
///
/// We first try the user-visible caption, then fall back to the raw file name
/// for older backups that still stored the extension.
pub fn lookup_icon_position_for_path(saved: &SavedIconLayout, path: &Path) -> Option<(i32, i32)> {
    let display_name = display_name_from_path(path)?;
    lookup_icon_position(saved, &display_name).or_else(|| {
        path.file_name()
            .and_then(|name| lookup_icon_position(saved, &name.to_string_lossy()))
    })
}

/// Set a single desktop icon's position by display name.
///
/// Acquires the desktop `IFolderView` via COM and positions the named icon
/// at the given coordinates. This is used to restore an icon's original
/// position after it is removed from a zone and restored to the Desktop.
pub fn set_single_icon_position(name: &str, x: i32, y: i32) -> Result<(), BentoDeskError> {
    tracing::info!("Setting icon position for '{}' to ({}, {})", name, x, y);

    let (_guard, folder_view) = finder::find_desktop_folder_view()?;
    writer::set_icon_position_by_name(&folder_view, name, x, y)
}

/// Restore a desktop icon position using the visible caption first and the raw
/// file name as a compatibility fallback.
pub fn set_single_icon_position_for_path(
    path: &Path,
    x: i32,
    y: i32,
) -> Result<(), BentoDeskError> {
    let Some(display_name) = display_name_from_path(path) else {
        return Err(BentoDeskError::IconPositionError(
            "path has no file name".to_string(),
        ));
    };

    match set_single_icon_position(&display_name, x, y) {
        Ok(()) => Ok(()),
        Err(error) => {
            let Some(raw_file_name) = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
            else {
                return Err(error);
            };
            if raw_file_name == display_name {
                return Err(error);
            }
            tracing::debug!(
                "Primary desktop-icon restore failed for '{}', retrying with raw name '{}': {}",
                display_name,
                raw_file_name,
                error
            );
            set_single_icon_position(&raw_file_name, x, y)
        }
    }
}

/// Return the path where the icon layout backup file would live for a given
/// data directory. Used by tests and recovery to locate the backup file.
pub fn backup_file_path(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join(BACKUP_FILENAME)
}

/// Resolve the BentoDesk data directory for icon layout persistence.
pub fn data_dir(handle: &AppHandle) -> std::path::PathBuf {
    crate::storage::state_data_dir(handle)
}

/// Resolve the fallback BentoDesk data directory for icon layout persistence.
///
/// Use this only when no live [`AppHandle`] is available.
pub fn default_data_dir() -> std::path::PathBuf {
    crate::storage::fallback_state_data_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_position_serialization_roundtrip() {
        let pos = IconPosition {
            name: "Test File.txt".to_string(),
            x: 100,
            y: 200,
        };
        let json = serde_json::to_string(&pos).unwrap();
        let parsed: IconPosition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test File.txt");
        assert_eq!(parsed.x, 100);
        assert_eq!(parsed.y, 200);
    }

    #[test]
    fn saved_layout_serialization_roundtrip() {
        let layout = SavedIconLayout {
            icons: vec![
                IconPosition {
                    name: "Recycle Bin".to_string(),
                    x: 0,
                    y: 0,
                },
                IconPosition {
                    name: "document.pdf".to_string(),
                    x: 75,
                    y: 0,
                },
            ],
            saved_at: "2026-03-22T09:00:00Z".to_string(),
            resolution: resolution::Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.25,
        };

        let json = serde_json::to_string_pretty(&layout).unwrap();
        let parsed: SavedIconLayout = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.icons.len(), 2);
        assert_eq!(parsed.icons[0].name, "Recycle Bin");
        assert_eq!(parsed.icons[1].x, 75);
        assert_eq!(parsed.resolution.width, 1920);
        assert!((parsed.dpi - 1.25).abs() < f64::EPSILON);
    }

    #[test]
    fn persist_and_load_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let layout = SavedIconLayout {
            icons: vec![IconPosition {
                name: "test.txt".to_string(),
                x: 42,
                y: 84,
            }],
            saved_at: "2026-03-22T09:00:00Z".to_string(),
            resolution: resolution::Resolution {
                width: 2560,
                height: 1440,
            },
            dpi: 1.5,
        };

        persist_to_file(&layout, dir.path()).unwrap();
        let loaded = load_from_file(dir.path()).unwrap().unwrap();

        assert_eq!(loaded.icons.len(), 1);
        assert_eq!(loaded.icons[0].name, "test.txt");
        assert_eq!(loaded.icons[0].x, 42);
    }

    #[test]
    fn display_name_from_path_strips_shortcut_extensions() {
        let shortcut = Path::new("C:\\Users\\HP\\Desktop\\Docs.lnk");
        let url = Path::new("C:\\Users\\HP\\Desktop\\Portal.url");
        let regular = Path::new("C:\\Users\\HP\\Desktop\\notes.txt");

        assert_eq!(display_name_from_path(shortcut).as_deref(), Some("Docs"));
        assert_eq!(display_name_from_path(url).as_deref(), Some("Portal"));
        assert_eq!(
            display_name_from_path(regular).as_deref(),
            Some("notes.txt")
        );
    }

    #[test]
    fn lookup_icon_position_for_path_falls_back_to_raw_file_name() {
        let saved = SavedIconLayout {
            icons: vec![IconPosition {
                name: "Docs.lnk".to_string(),
                x: 10,
                y: 20,
            }],
            saved_at: "2026-04-22T00:00:00Z".to_string(),
            resolution: resolution::Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
        };

        let position =
            lookup_icon_position_for_path(&saved, Path::new("C:\\Users\\HP\\Desktop\\Docs.lnk"));
        assert_eq!(position, Some((10, 20)));
    }

    #[test]
    fn load_from_nonexistent_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_from_file(dir.path()).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn restore_empty_layout_is_noop() {
        let empty = SavedIconLayout {
            icons: vec![],
            saved_at: "2026-03-22T09:00:00Z".to_string(),
            resolution: resolution::Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
        };
        // restore_layout with empty icons should return Ok immediately
        let result = restore_layout(&empty);
        assert!(result.is_ok());
    }
}
