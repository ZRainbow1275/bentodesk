//! T-086 — Desktop icon position save/restore (lift-port from 1.x
//! `src-tauri/src/icon_positions/`).
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
//! # What changed vs 1.x
//!
//! | 1.x                                     | nano                                                                      |
//! |-----------------------------------------|---------------------------------------------------------------------------|
//! | `tauri::AppHandle` for path resolution  | caller passes `&Path` for the data dir; nano backend has no AppHandle     |
//! | `crate::storage::state_data_dir(handle)`| caller resolves via `bento_nano_backend::storage::state_data_dir(...)`    |
//! | `chrono::Utc::now().to_rfc3339()`       | [`crate::time::now_rfc3339`] — hand-rolled, no chrono dep                 |
//! | `crate::layout::resolution::Resolution` | [`Resolution`] inlined here — small enough not to need its own module     |
//! | `BentoDeskError::IconPositionError`     | hand-rolled [`IconPositionError`] enum (spec §8.1, no thiserror)          |
//!
//! # Usage
//! ```rust,ignore
//! // On startup:
//! let layout = icon_positions::save_layout()?;
//! icon_positions::persist_to_file(&layout, &data_dir)?;
//! // On exit:
//! if let Some(saved) = icon_positions::load_from_file(&data_dir)? {
//!     icon_positions::restore_layout(&saved)?;
//! }
//! ```

pub mod finder;
pub mod reader;
pub mod writer;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use writer::RestoreResult;

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the icon-position module.
///
/// Carries enough structure for a caller to render a useful message without
/// exposing raw `windows::core::Error` / `serde_json::Error` types across the
/// public API (which would force every downstream caller to add the same deps).
#[derive(Debug)]
pub enum IconPositionError {
    /// A COM call (`CoCreateInstance` / `IFolderView::*` / `IShellFolder::*` /
    /// `StrRetToStrW`) returned a non-`S_OK` HRESULT.
    Com {
        /// Static label naming the COM call site, e.g. `"IFolderView::Items"`.
        ctx: &'static str,
        /// HRESULT-formatted message body lifted from `windows::core::Error`.
        message: String,
    },
    /// Auto-arrange was on and we could not turn it off; restore would not
    /// produce stable positions, so we abort early.
    AutoArrangeLocked,
    /// I/O while reading or writing the JSON backup file failed.
    Io { path: PathBuf, message: String },
    /// JSON serialize / deserialize of the saved layout failed.
    Parse { path: PathBuf, message: String },
    /// Caller passed a path that has no file-name component (e.g. `/`).
    InvalidPath { reason: &'static str },
}

impl core::fmt::Display for IconPositionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Com { ctx, message } => write!(f, "{ctx}: {message}"),
            Self::AutoArrangeLocked => f.write_str(
                "cannot restore icon positions: auto-arrange is enabled and could not be disabled",
            ),
            Self::Io { path, message } => {
                write!(
                    f,
                    "icon-position io error at {}: {}",
                    path.display(),
                    message
                )
            }
            Self::Parse { path, message } => {
                write!(
                    f,
                    "icon-position parse error at {}: {}",
                    path.display(),
                    message
                )
            }
            Self::InvalidPath { reason } => write!(f, "invalid path: {reason}"),
        }
    }
}

impl core::error::Error for IconPositionError {}

// ─── Schema (byte-equivalent to 1.x JSON) ────────────────────────────

/// Screen resolution snapshot captured alongside an icon layout.
///
/// Inlined here (rather than imported from a `layout::resolution` module that
/// does not yet exist in the nano backend) to keep T-086 self-contained per
/// the master plan §6 "lift verbatim, minimal coupling" rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

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
    /// RFC3339 timestamp when this snapshot was taken (UTC, ms precision).
    pub saved_at: String,
    /// Screen resolution at the time of capture.
    pub resolution: Resolution,
    /// DPI scale factor at the time of capture (e.g. 1.0 = 96 DPI).
    pub dpi: f64,
}

/// The default backup filename stored in the BentoDesk data directory.
const BACKUP_FILENAME: &str = "icon_layout_backup.json";

// ─── Display / DPI helpers (inlined, see Resolution above) ──────────

/// Detect the current primary monitor resolution.
pub fn current_resolution() -> Resolution {
    // SAFETY: GetSystemMetrics is documented infallible for SM_CXSCREEN and
    // SM_CYSCREEN; it returns 0 if the system has no display, which we
    // surface as `Resolution { 0, 0 }`.
    let width = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_CXSCREEN,
        )
    };
    // SAFETY: same as above.
    let height = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN,
        )
    };
    Resolution {
        width: width.max(0) as u32,
        height: height.max(0) as u32,
    }
}

/// Get the current DPI scale factor for the primary monitor.
///
/// Returns a multiplier (1.0 = 96 DPI, 1.25 = 120 DPI, 1.5 = 144 DPI).
pub fn dpi_scale() -> f64 {
    // Mc-1a — DPI soft-loaded via `crate::dpi_compat` (GetProcAddress). This
    // previously used the `windows` crate's static `GetDpiForSystem`, which
    // created a PE import absent on Win10 <1607 / 8.1 / 7; swapping it also
    // drops that static import.
    let dpi = crate::dpi_compat::system_dpi();
    dpi as f64 / 96.0
}

// ─── Public API ──────────────────────────────────────────────────────

/// Save the current desktop icon layout.
///
/// Acquires the desktop `IFolderView` via COM, enumerates all icons,
/// and returns a [`SavedIconLayout`] with their names and positions.
///
/// This function initializes COM (STA) for the duration of the call.
pub fn save_layout() -> Result<SavedIconLayout, IconPositionError> {
    tracing::info!("Saving desktop icon positions...");

    let (_guard, folder_view) = finder::find_desktop_folder_view()?;
    let icons = reader::read_all_icon_positions(&folder_view)?;

    let layout = SavedIconLayout {
        icons,
        saved_at: crate::time::now_rfc3339(),
        resolution: current_resolution(),
        dpi: dpi_scale(),
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
pub fn restore_layout(saved: &SavedIconLayout) -> Result<RestoreResult, IconPositionError> {
    if saved.icons.is_empty() {
        tracing::warn!("No icon positions to restore (empty backup)");
        return Ok(RestoreResult::default());
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

    Ok(result)
}

/// Save the current icon layout to a JSON file in the given data directory.
///
/// The file is written to `{data_dir}/icon_layout_backup.json`.
pub fn persist_to_file(layout: &SavedIconLayout, data_dir: &Path) -> Result<(), IconPositionError> {
    std::fs::create_dir_all(data_dir).map_err(|e| IconPositionError::Io {
        path: data_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let path = data_dir.join(BACKUP_FILENAME);
    let json = serde_json::to_string_pretty(layout).map_err(|e| IconPositionError::Parse {
        path: path.clone(),
        message: e.to_string(),
    })?;
    std::fs::write(&path, &json).map_err(|e| IconPositionError::Io {
        path: path.clone(),
        message: e.to_string(),
    })?;
    tracing::debug!("Icon layout backup written to {}", path.display());
    Ok(())
}

/// Load a previously saved icon layout from JSON in the given data directory.
///
/// Returns `Ok(None)` if the backup file does not exist.
pub fn load_from_file(data_dir: &Path) -> Result<Option<SavedIconLayout>, IconPositionError> {
    let path = data_dir.join(BACKUP_FILENAME);
    if !path.exists() {
        tracing::debug!("No icon layout backup found at {}", path.display());
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path).map_err(|e| IconPositionError::Io {
        path: path.clone(),
        message: e.to_string(),
    })?;
    let layout: SavedIconLayout =
        serde_json::from_str(&json).map_err(|e| IconPositionError::Parse {
            path: path.clone(),
            message: e.to_string(),
        })?;
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
pub fn set_single_icon_position(name: &str, x: i32, y: i32) -> Result<(), IconPositionError> {
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
) -> Result<(), IconPositionError> {
    let Some(display_name) = display_name_from_path(path) else {
        return Err(IconPositionError::InvalidPath {
            reason: "path has no file name",
        });
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
                error,
            );
            set_single_icon_position(&raw_file_name, x, y)
        }
    }
}

/// Return the path where the icon layout backup file would live for a given
/// data directory. Used by tests and recovery to locate the backup file.
pub fn backup_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUP_FILENAME)
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
        let json = serde_json::to_string(&pos).expect("serialize");
        let parsed: IconPosition = serde_json::from_str(&json).expect("parse");
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
            saved_at: "2026-03-22T09:00:00.000Z".to_string(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.25,
        };

        let json = serde_json::to_string_pretty(&layout).expect("serialize");
        let parsed: SavedIconLayout = serde_json::from_str(&json).expect("parse");

        assert_eq!(parsed.icons.len(), 2);
        assert_eq!(parsed.icons[0].name, "Recycle Bin");
        assert_eq!(parsed.icons[1].x, 75);
        assert_eq!(parsed.resolution.width, 1920);
        assert!((parsed.dpi - 1.25).abs() < f64::EPSILON);
    }

    #[test]
    fn persist_and_load_from_file() {
        let dir = tempdir();
        let layout = SavedIconLayout {
            icons: vec![IconPosition {
                name: "test.txt".to_string(),
                x: 42,
                y: 84,
            }],
            saved_at: "2026-03-22T09:00:00.000Z".to_string(),
            resolution: Resolution {
                width: 2560,
                height: 1440,
            },
            dpi: 1.5,
        };

        persist_to_file(&layout, &dir).expect("persist");
        let loaded = load_from_file(&dir).expect("load").expect("some");

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
            saved_at: "2026-04-22T00:00:00.000Z".to_string(),
            resolution: Resolution {
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
    fn lookup_icon_position_three_tier_identity_fallback() {
        // Layout-restore identity contract: when restoring an item that was
        // previously hidden inside `.bentodesk/`, the caller must be able to
        // resolve the saved icon position from any of the three persisted
        // identifiers.
        //   tier 1 — original_path: the desktop path the file was hiding from
        //   tier 2 — hidden_path:   the path inside `.bentodesk/{zone}/`
        //   tier 3 — display_name:  the visible caption (extension-stripped)
        let saved = SavedIconLayout {
            icons: vec![
                IconPosition {
                    name: "Quarterly Plan".to_string(),
                    x: 100,
                    y: 200,
                },
                IconPosition {
                    name: "report.pdf".to_string(),
                    x: 50,
                    y: 75,
                },
            ],
            saved_at: "2026-04-22T00:00:00.000Z".to_string(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
        };

        let original = Path::new("C:\\Users\\HP\\Desktop\\Quarterly Plan.lnk");
        assert_eq!(
            lookup_icon_position_for_path(&saved, original),
            Some((100, 200)),
            "tier 1 (original_path) failed",
        );

        let hidden =
            Path::new("C:\\Users\\HP\\AppData\\BentoDesk\\.bentodesk\\zone-a\\Quarterly Plan.lnk");
        assert_eq!(
            lookup_icon_position_for_path(&saved, hidden),
            Some((100, 200)),
            "tier 2 (hidden_path) failed",
        );

        let display_only = Path::new("report.pdf");
        assert_eq!(
            display_name_from_path(display_only).as_deref(),
            Some("report.pdf"),
            "tier 3 (display_name derivation) failed",
        );
        assert_eq!(
            lookup_icon_position(&saved, "report.pdf"),
            Some((50, 75)),
            "tier 3 (display_name lookup) failed",
        );
    }

    #[test]
    fn load_from_nonexistent_returns_none() {
        let dir = tempdir();
        let loaded = load_from_file(&dir).expect("load");
        assert!(loaded.is_none());
    }

    #[test]
    fn restore_empty_layout_is_noop() {
        let empty = SavedIconLayout {
            icons: vec![],
            saved_at: "2026-03-22T09:00:00.000Z".to_string(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
        };
        let result = restore_layout(&empty);
        assert!(result.is_ok());
        let r = result.expect("ok");
        assert_eq!(r.restored, 0);
        assert_eq!(r.failed, 0);
    }

    #[test]
    fn backup_file_path_has_filename() {
        let dir = Path::new("C:\\Users\\HP\\AppData");
        let p = backup_file_path(dir);
        assert!(p.ends_with("icon_layout_backup.json"));
    }

    /// Per-process unique temp directory rooted under the OS temp dir.
    ///
    /// Replaces the 1.x `tempfile::tempdir()` dependency (`tempfile` is not
    /// on the §8 whitelist). Cleanup is best-effort; tests that allocate a
    /// directory remove their files individually.
    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("bentonano-icon-pos-{pid}-{n}"));
        std::fs::create_dir_all(&dir).expect("create tempdir");
        dir
    }
}
