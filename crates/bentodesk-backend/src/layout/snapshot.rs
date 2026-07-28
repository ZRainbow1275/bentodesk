//! T-097 — desktop layout snapshot manager.
//!
//! Plain `std::fs` save/load/list/delete over a directory of
//! `snapshot-{id}.json` files. Errors surface through the shared
//! [`super::persistence::LayoutError`] enum so the dispatcher can pattern
//! match `Io` vs `Serde` failures.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use super::persistence::{BentoZone, LayoutError};
use super::resolution::Resolution;

/// A complete snapshot of the desktop layout at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DesktopSnapshot {
    pub id: SmolStr,
    pub name: String,
    pub resolution: Resolution,
    pub dpi: f64,
    pub zones: Vec<BentoZone>,
    pub captured_at: SmolStr,
}

/// Manager for layout snapshots, scoped to a directory chosen by the caller.
pub struct SnapshotManager {
    snapshots_dir: PathBuf,
}

impl SnapshotManager {
    /// Create a new snapshot manager with the given directory. The directory
    /// is created lazily on first [`save`] — `new` performs no I/O.
    pub fn new(snapshots_dir: PathBuf) -> Self {
        Self { snapshots_dir }
    }

    /// Persist a snapshot to disk as `snapshot-{snapshot.id}.json`.
    pub fn save(&self, snapshot: &DesktopSnapshot) -> Result<(), LayoutError> {
        std::fs::create_dir_all(&self.snapshots_dir).map_err(|e| LayoutError::Io {
            path: self.snapshots_dir.clone(),
            source: e,
        })?;
        let path = self.path_for(&snapshot.id);
        let bytes = serde_json::to_vec_pretty(snapshot).map_err(|e| LayoutError::Serde {
            path: path.clone(),
            source: e,
        })?;
        std::fs::write(&path, &bytes).map_err(|e| LayoutError::Io {
            path: path.clone(),
            source: e,
        })
    }

    /// Load a snapshot by ID. Returns `Err` if the file is missing or
    /// corrupt — caller decides whether to fall back.
    pub fn load(&self, id: &str) -> Result<DesktopSnapshot, LayoutError> {
        let path = self.path_for(id);
        let bytes = std::fs::read(&path).map_err(|e| LayoutError::Io {
            path: path.clone(),
            source: e,
        })?;
        serde_json::from_slice(&bytes).map_err(|e| LayoutError::Serde {
            path: path.clone(),
            source: e,
        })
    }

    /// List all saved snapshots, newest first by `captured_at`. Skips files
    /// whose JSON failed to parse so a single bad snapshot doesn't poison
    /// the list (1.x behaviour preserved).
    pub fn list(&self) -> Result<Vec<DesktopSnapshot>, LayoutError> {
        let mut snapshots = Vec::new();
        if !self.snapshots_dir.exists() {
            return Ok(snapshots);
        }
        let read_dir = std::fs::read_dir(&self.snapshots_dir).map_err(|e| LayoutError::Io {
            path: self.snapshots_dir.clone(),
            source: e,
        })?;
        for entry in read_dir {
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!("snapshot list: read_dir entry failed: {err}");
                    continue;
                }
            };
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let bytes = match std::fs::read(&path) {
                    Ok(b) => b,
                    Err(err) => {
                        tracing::warn!(
                            "snapshot list: skipping unreadable file {}: {err}",
                            path.display()
                        );
                        continue;
                    }
                };
                if let Ok(snapshot) = serde_json::from_slice::<DesktopSnapshot>(&bytes) {
                    snapshots.push(snapshot);
                } else {
                    tracing::warn!("snapshot list: skipping corrupt file {}", path.display());
                }
            }
        }
        snapshots.sort_by(|a, b| b.captured_at.cmp(&a.captured_at));
        Ok(snapshots)
    }

    /// Delete a snapshot by ID. Idempotent — no error when the file is
    /// already absent.
    pub fn delete(&self, id: &str) -> Result<(), LayoutError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Ok(());
        }
        std::fs::remove_file(&path).map_err(|e| LayoutError::Io {
            path: path.clone(),
            source: e,
        })
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.snapshots_dir.join(format!("snapshot-{id}.json"))
    }

    /// Snapshot directory the manager was constructed with.
    pub fn dir(&self) -> &Path {
        &self.snapshots_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Hand-rolled scratch dir under `%TEMP%\bentodesk-snap-{tid}-{n}`.
    /// Avoids the `tempfile` crate (forbidden — not on §8 whitelist) while
    /// still giving every test a fresh isolated directory.
    fn scratch_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let tid = std::thread::current().id();
        let path = std::env::temp_dir().join(format!("bentodesk-snap-{tid:?}-{n}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    fn make_test_snapshot(id: &str, name: &str, captured_at: &str) -> DesktopSnapshot {
        DesktopSnapshot {
            id: SmolStr::from(id),
            name: name.to_string(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
            zones: Vec::new(),
            captured_at: SmolStr::from(captured_at),
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = scratch_dir();
        let manager = SnapshotManager::new(dir.clone());
        let snapshot = make_test_snapshot("test-001", "My Layout", "2026-01-01T00:00:00Z");
        manager.save(&snapshot).expect("save");
        let loaded = manager.load("test-001").expect("load");
        assert_eq!(loaded, snapshot);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_returns_empty_for_nonexistent_dir() {
        let dir = scratch_dir().join("nonexistent");
        let manager = SnapshotManager::new(dir);
        let list = manager.list().expect("list");
        assert!(list.is_empty());
    }

    #[test]
    fn list_returns_all_snapshots_sorted_newest_first() {
        let dir = scratch_dir();
        let manager = SnapshotManager::new(dir.clone());
        let older = make_test_snapshot("snap-old", "Old", "2026-01-01T00:00:00Z");
        let newer = make_test_snapshot("snap-new", "New", "2026-06-15T12:00:00Z");
        manager.save(&older).expect("save older");
        manager.save(&newer).expect("save newer");
        let list = manager.list().expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id.as_str(), "snap-new");
        assert_eq!(list[1].id.as_str(), "snap-old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_existing_snapshot() {
        let dir = scratch_dir();
        let manager = SnapshotManager::new(dir.clone());
        let snapshot = make_test_snapshot("to-delete", "Deletable", "2026-01-01T00:00:00Z");
        manager.save(&snapshot).expect("save");
        assert!(manager.load("to-delete").is_ok());
        manager.delete("to-delete").expect("delete");
        assert!(manager.load("to-delete").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_nonexistent_is_idempotent() {
        let dir = scratch_dir();
        let manager = SnapshotManager::new(dir.clone());
        manager.delete("does-not-exist").expect("idempotent delete");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_nonexistent_returns_io_error() {
        let dir = scratch_dir();
        let manager = SnapshotManager::new(dir.clone());
        let err = manager.load("missing").expect_err("must fail");
        assert!(matches!(err, LayoutError::Io { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_skips_corrupt_files_without_failing() {
        let dir = scratch_dir();
        let manager = SnapshotManager::new(dir.clone());
        let good = make_test_snapshot("snap-good", "Good", "2026-01-01T00:00:00Z");
        manager.save(&good).expect("save good");
        std::fs::write(dir.join("snapshot-corrupt.json"), b"{ broken }")
            .expect("write corrupt fixture");
        let list = manager.list().expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id.as_str(), "snap-good");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_round_trips_via_serde() {
        let snapshot = make_test_snapshot("ser-test", "Serde", "2026-05-01T12:00:00Z");
        let json = serde_json::to_string(&snapshot).expect("ser");
        let parsed: DesktopSnapshot = serde_json::from_str(&json).expect("de");
        assert_eq!(parsed, snapshot);
    }
}
