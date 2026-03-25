use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::persistence::BentoZone;
use super::resolution::Resolution;
use crate::error::BentoDeskError;

/// A complete snapshot of the desktop layout at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopSnapshot {
    pub id: String,
    pub name: String,
    pub resolution: Resolution,
    pub dpi: f64,
    pub zones: Vec<BentoZone>,
    pub captured_at: String,
}

/// Manager for layout snapshots.
pub struct SnapshotManager {
    snapshots_dir: PathBuf,
}

impl SnapshotManager {
    /// Create a new snapshot manager with the given directory.
    pub fn new(snapshots_dir: PathBuf) -> Self {
        Self { snapshots_dir }
    }

    /// Save a snapshot to disk.
    pub fn save(&self, snapshot: &DesktopSnapshot) -> Result<(), BentoDeskError> {
        std::fs::create_dir_all(&self.snapshots_dir)?;
        let filename = format!("snapshot-{}.json", snapshot.id);
        let path = self.snapshots_dir.join(filename);
        let content = serde_json::to_string_pretty(snapshot)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load a snapshot by ID.
    pub fn load(&self, id: &str) -> Result<DesktopSnapshot, BentoDeskError> {
        let filename = format!("snapshot-{id}.json");
        let path = self.snapshots_dir.join(filename);
        let content = std::fs::read_to_string(path)?;
        let snapshot: DesktopSnapshot = serde_json::from_str(&content)?;
        Ok(snapshot)
    }

    /// List all saved snapshots.
    pub fn list(&self) -> Result<Vec<DesktopSnapshot>, BentoDeskError> {
        let mut snapshots = Vec::new();
        if !self.snapshots_dir.exists() {
            return Ok(snapshots);
        }
        for entry in std::fs::read_dir(&self.snapshots_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let content = std::fs::read_to_string(&path)?;
                if let Ok(snapshot) = serde_json::from_str::<DesktopSnapshot>(&content) {
                    snapshots.push(snapshot);
                }
            }
        }
        snapshots.sort_by(|a, b| b.captured_at.cmp(&a.captured_at));
        Ok(snapshots)
    }

    /// Delete a snapshot by ID.
    pub fn delete(&self, id: &str) -> Result<(), BentoDeskError> {
        let filename = format!("snapshot-{id}.json");
        let path = self.snapshots_dir.join(filename);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_test_snapshot(id: &str, name: &str) -> DesktopSnapshot {
        DesktopSnapshot {
            id: id.to_string(),
            name: name.to_string(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
            zones: Vec::new(),
            captured_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SnapshotManager::new(dir.path().to_path_buf());

        let snapshot = make_test_snapshot("test-001", "My Layout");
        manager.save(&snapshot).unwrap();

        let loaded = manager.load("test-001").unwrap();
        assert_eq!(loaded.id, "test-001");
        assert_eq!(loaded.name, "My Layout");
        assert_eq!(loaded.resolution.width, 1920);
        assert_eq!(loaded.dpi, 1.0);
    }

    #[test]
    fn list_returns_empty_for_nonexistent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SnapshotManager::new(dir.path().join("nonexistent"));
        let result = manager.list().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn list_returns_all_snapshots_sorted_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SnapshotManager::new(dir.path().to_path_buf());

        let older = DesktopSnapshot {
            captured_at: "2026-01-01T00:00:00Z".to_string(),
            ..make_test_snapshot("snap-old", "Old")
        };
        let newer = DesktopSnapshot {
            captured_at: "2026-06-15T12:00:00Z".to_string(),
            ..make_test_snapshot("snap-new", "New")
        };

        manager.save(&older).unwrap();
        manager.save(&newer).unwrap();

        let list = manager.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "snap-new"); // Newest first
        assert_eq!(list[1].id, "snap-old");
    }

    #[test]
    fn delete_existing_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SnapshotManager::new(dir.path().to_path_buf());

        let snapshot = make_test_snapshot("to-delete", "Deletable");
        manager.save(&snapshot).unwrap();
        assert!(manager.load("to-delete").is_ok());

        manager.delete("to-delete").unwrap();
        assert!(manager.load("to-delete").is_err());
    }

    #[test]
    fn delete_nonexistent_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SnapshotManager::new(dir.path().to_path_buf());
        // Deleting a non-existent snapshot should succeed
        assert!(manager.delete("does-not-exist").is_ok());
    }

    #[test]
    fn load_nonexistent_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SnapshotManager::new(dir.path().to_path_buf());
        assert!(manager.load("missing").is_err());
    }
}
