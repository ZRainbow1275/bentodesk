//! BentoDesk unified error types.
//!
//! All backend modules use [`BentoDeskError`] for error propagation.
//! Tauri commands convert errors to `String` via the [`serde::Serialize`] impl.

use thiserror::Error;

/// Top-level error type for all BentoDesk backend operations.
#[derive(Debug, Error)]
pub enum BentoDeskError {
    /// Ghost layer initialization or detachment failed.
    #[error("Ghost layer initialization failed: {0}")]
    GhostLayerError(String),

    /// Icon extraction failed for the specified path.
    #[error("Icon extraction failed for {path}: {source}")]
    IconError {
        path: String,
        #[source]
        source: windows::core::Error,
    },

    /// File watcher encountered an error.
    #[error("File watcher error: {0}")]
    WatcherError(#[from] notify::Error),

    /// Layout persistence (JSON serialization/deserialization) error.
    #[error("Layout persistence error: {0}")]
    SerdeError(#[from] serde_json::Error),

    /// Generic I/O error (file read/write, directory creation, etc.).
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// COM subsystem error (OLE drag-drop, icon extraction).
    #[error("COM error: {0}")]
    ComError(#[from] windows::core::Error),

    /// OLE drag operation failed.
    #[error("Drag operation failed: {0}")]
    DragError(String),

    /// Configuration error (load/save).
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Zone not found by ID.
    #[error("Zone not found: {0}")]
    ZoneNotFound(String),

    /// Item not found by ID within a zone.
    #[error("Item not found: {0}")]
    ItemNotFound(String),

    /// Snapshot not found by ID.
    #[error("Snapshot not found: {0}")]
    SnapshotNotFound(String),

    /// Image encoding error during icon extraction.
    #[error("Image encoding error: {0}")]
    ImageError(String),

    /// Desktop icon position save/restore error.
    #[error("Icon position error: {0}")]
    IconPositionError(String),

    /// Generic application error.
    #[error("{0}")]
    Generic(String),
}

/// Serialize error as string for Tauri IPC transport.
impl serde::Serialize for BentoDeskError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_not_found_serializes_to_expected_string() {
        let err = BentoDeskError::ZoneNotFound("abc-123".into());
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(json, "\"Zone not found: abc-123\"");
    }

    #[test]
    fn item_not_found_serializes_to_expected_string() {
        let err = BentoDeskError::ItemNotFound("item-456".into());
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(json, "\"Item not found: item-456\"");
    }

    #[test]
    fn snapshot_not_found_serializes_to_expected_string() {
        let err = BentoDeskError::SnapshotNotFound("snap-789".into());
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(json, "\"Snapshot not found: snap-789\"");
    }

    #[test]
    fn ghost_layer_error_display() {
        let err = BentoDeskError::GhostLayerError("Main window not found".into());
        assert_eq!(
            err.to_string(),
            "Ghost layer initialization failed: Main window not found"
        );
    }

    #[test]
    fn drag_error_display() {
        let err = BentoDeskError::DragError("No files to drag".into());
        assert_eq!(err.to_string(), "Drag operation failed: No files to drag");
    }

    #[test]
    fn config_error_display() {
        let err = BentoDeskError::ConfigError("Cannot determine Desktop path".into());
        assert_eq!(
            err.to_string(),
            "Configuration error: Cannot determine Desktop path"
        );
    }

    #[test]
    fn generic_error_display() {
        let err = BentoDeskError::Generic("something went wrong".into());
        assert_eq!(err.to_string(), "something went wrong");
    }

    #[test]
    fn io_error_converts_via_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: BentoDeskError = io_err.into();
        assert!(matches!(err, BentoDeskError::IoError(_)));
        assert!(err.to_string().contains("file missing"));
    }

    #[test]
    fn serde_error_converts_via_from() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let err: BentoDeskError = json_err.into();
        assert!(matches!(err, BentoDeskError::SerdeError(_)));
    }

    #[test]
    fn image_error_serializes_correctly() {
        let err = BentoDeskError::ImageError("PNG encoding failed".into());
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(json, "\"Image encoding error: PNG encoding failed\"");
    }
}
