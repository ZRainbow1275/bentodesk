//! In-memory index over the on-disk checkpoint store.
//!
//! Holds two lists:
//! - `auto`: bounded ring of auto-captured snapshots (default 20 slots). When
//!   full, the oldest entry is evicted from disk + memory.
//! - `pinned`: unbounded list of manual / permanent checkpoints.
//!
//! The [`TimelineBuffer`] is held inside an `std::sync::Mutex` on `AppState`
//! and mutated synchronously from the Tauri command layer. Disk I/O happens
//! inside `push_auto` / `pin` / `restore_cursor_to` after the in-memory mutation.

use std::collections::VecDeque;

use super::checkpoint::{Checkpoint, CheckpointMeta, CheckpointStore};

/// Default auto-capture retention.
pub const DEFAULT_AUTO_CAPACITY: usize = 20;

pub struct TimelineBuffer {
    pub auto: VecDeque<Checkpoint>,
    pub pinned: Vec<Checkpoint>,
    pub auto_capacity: usize,
    /// Cursor into the merged sorted list used for undo/redo. `None` means
    /// "at head" (latest), otherwise the index inside the merged timeline.
    pub cursor: Option<usize>,
}

impl Default for TimelineBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_AUTO_CAPACITY)
    }
}

impl TimelineBuffer {
    pub fn new(auto_capacity: usize) -> Self {
        Self {
            auto: VecDeque::with_capacity(auto_capacity),
            pinned: Vec::new(),
            auto_capacity: auto_capacity.max(1),
            cursor: None,
        }
    }

    /// Rebuild the buffer from disk. Pinned checkpoints always load; auto
    /// checkpoints load newest-first up to `auto_capacity`.
    pub fn reload(&mut self, store: &CheckpointStore) {
        let mut all = store.load_all();
        // load_all sorts ascending; newest last.
        let (mut pinned, mut autos): (Vec<_>, Vec<_>) = all.drain(..).partition(|cp| cp.pinned);
        // Keep only the newest `auto_capacity` auto entries.
        if autos.len() > self.auto_capacity {
            let drop_n = autos.len() - self.auto_capacity;
            for stale in autos.drain(..drop_n) {
                if let Err(e) = store.delete(&stale.id) {
                    tracing::warn!(
                        "Timeline: failed to evict stale auto checkpoint {}: {}",
                        stale.id,
                        e
                    );
                }
            }
        }
        self.auto = VecDeque::from(autos);
        pinned.sort_by(|a, b| a.snapshot.captured_at.cmp(&b.snapshot.captured_at));
        self.pinned = pinned;
        self.cursor = None;
    }

    /// Push a new auto checkpoint; evicts the oldest if at capacity.
    /// Also resets the undo cursor so the user sees the latest state.
    pub fn push_auto(&mut self, store: &CheckpointStore, cp: Checkpoint) {
        if let Err(e) = store.save(&cp) {
            tracing::warn!("Timeline: failed to persist checkpoint {}: {}", cp.id, e);
            return;
        }
        self.auto.push_back(cp);
        while self.auto.len() > self.auto_capacity {
            if let Some(stale) = self.auto.pop_front() {
                if let Err(e) = store.delete(&stale.id) {
                    tracing::warn!(
                        "Timeline: failed to evict auto checkpoint {}: {}",
                        stale.id,
                        e
                    );
                }
            }
        }
        self.cursor = None;
    }

    /// Pin a checkpoint (by id) so it is never auto-evicted. If the id is not
    /// yet stored (e.g. ad-hoc manual save), caller should use [`push_pinned`]
    /// instead.
    pub fn pin(&mut self, store: &CheckpointStore, id: &str) -> Option<Checkpoint> {
        if let Some(pos) = self.auto.iter().position(|cp| cp.id == id) {
            let mut cp = self
                .auto
                .remove(pos)
                .expect("position returned a valid index");
            cp.pinned = true;
            if let Err(e) = store.save(&cp) {
                tracing::warn!("Timeline: failed to save pinned checkpoint: {}", e);
            }
            self.pinned.push(cp.clone());
            self.pinned
                .sort_by(|a, b| a.snapshot.captured_at.cmp(&b.snapshot.captured_at));
            return Some(cp);
        }
        None
    }

    /// Insert a newly-created pinned checkpoint (manual save).
    pub fn push_pinned(&mut self, store: &CheckpointStore, mut cp: Checkpoint) {
        cp.pinned = true;
        if let Err(e) = store.save(&cp) {
            tracing::warn!("Timeline: failed to save manual checkpoint: {}", e);
            return;
        }
        self.pinned.push(cp);
        self.pinned
            .sort_by(|a, b| a.snapshot.captured_at.cmp(&b.snapshot.captured_at));
    }

    /// Remove a checkpoint from both memory and disk.
    pub fn remove(&mut self, store: &CheckpointStore, id: &str) -> bool {
        let removed_auto = if let Some(pos) = self.auto.iter().position(|cp| cp.id == id) {
            self.auto.remove(pos).is_some()
        } else {
            false
        };
        let removed_pinned = if let Some(pos) = self.pinned.iter().position(|cp| cp.id == id) {
            self.pinned.remove(pos);
            true
        } else {
            false
        };
        if removed_auto || removed_pinned {
            if let Err(e) = store.delete(id) {
                tracing::warn!("Timeline: failed to delete checkpoint {}: {}", id, e);
            }
            self.cursor = None;
            return true;
        }
        false
    }

    /// All checkpoints merged + sorted ascending by captured_at. Used for
    /// frontend listing and cursor-based navigation.
    pub fn merged(&self) -> Vec<&Checkpoint> {
        let mut v: Vec<&Checkpoint> = self.auto.iter().chain(self.pinned.iter()).collect();
        v.sort_by(|a, b| a.snapshot.captured_at.cmp(&b.snapshot.captured_at));
        v
    }

    pub fn metas(&self) -> Vec<CheckpointMeta> {
        self.merged().iter().map(|cp| (*cp).into()).collect()
    }

    /// Compute the previous checkpoint relative to the current cursor (for
    /// Ctrl+Z). Updates the cursor in place and returns the target.
    pub fn step_back(&mut self) -> Option<Checkpoint> {
        let merged_len = self.merged().len();
        if merged_len == 0 {
            return None;
        }
        // `cursor` None = at head (most recent). First step_back goes to
        // merged.len() - 2 (the state *before* the last checkpoint).
        let next_idx = match self.cursor {
            None => {
                if merged_len < 2 {
                    return None;
                }
                merged_len - 2
            }
            Some(0) => return None,
            Some(i) => i - 1,
        };
        let cp = self.merged()[next_idx].clone();
        self.cursor = Some(next_idx);
        Some(cp)
    }

    /// Redo (Ctrl+Shift+Z) — moves the cursor forward.
    pub fn step_forward(&mut self) -> Option<Checkpoint> {
        let merged_len = self.merged().len();
        if merged_len == 0 {
            return None;
        }
        let next_idx = match self.cursor {
            None => return None,
            Some(i) if i + 1 >= merged_len => {
                let last = self.merged().last().map(|cp| (*cp).clone());
                self.cursor = None;
                return last;
            }
            Some(i) => i + 1,
        };
        let cp = self.merged()[next_idx].clone();
        self.cursor = Some(next_idx);
        Some(cp)
    }

    /// Set the cursor to a specific checkpoint id (timeline slider drag).
    pub fn seek(&mut self, id: &str) -> Option<Checkpoint> {
        let (pos, cp) = {
            let merged = self.merged();
            let pos = merged.iter().position(|cp| cp.id == id)?;
            (pos, merged[pos].clone())
        };
        let merged_len = self.merged().len();
        self.cursor = if pos + 1 == merged_len {
            None
        } else {
            Some(pos)
        };
        Some(cp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::persistence::RelativePosition;
    use crate::layout::resolution::Resolution;
    use crate::layout::snapshot::DesktopSnapshot;
    use crate::timeline::checkpoint::DeltaSummary;

    fn make_cp(id: &str, ts: &str, pinned: bool) -> Checkpoint {
        Checkpoint {
            id: id.to_string(),
            snapshot: DesktopSnapshot {
                id: id.to_string(),
                name: id.to_string(),
                resolution: Resolution {
                    width: 1920,
                    height: 1080,
                },
                dpi: 1.0,
                zones: Vec::new(),
                captured_at: ts.to_string(),
            },
            delta: DeltaSummary::default(),
            delta_summary: String::new(),
            trigger: "test".to_string(),
            pinned,
        }
    }

    #[test]
    fn push_auto_evicts_oldest() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().to_path_buf());
        let mut buf = TimelineBuffer::new(3);
        for i in 0..5 {
            buf.push_auto(
                &store,
                make_cp(
                    &format!("c{i}"),
                    &format!("2026-01-0{}T00:00:00Z", i + 1),
                    false,
                ),
            );
        }
        assert_eq!(buf.auto.len(), 3);
        // Oldest two should be gone, keep c2..c4.
        assert_eq!(buf.auto.front().unwrap().id, "c2");
        assert_eq!(buf.auto.back().unwrap().id, "c4");
    }

    #[test]
    fn pinned_are_never_evicted() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().to_path_buf());
        let mut buf = TimelineBuffer::new(2);
        buf.push_pinned(&store, make_cp("p1", "2026-01-01T00:00:00Z", false));
        buf.push_auto(&store, make_cp("a1", "2026-01-02T00:00:00Z", false));
        buf.push_auto(&store, make_cp("a2", "2026-01-03T00:00:00Z", false));
        buf.push_auto(&store, make_cp("a3", "2026-01-04T00:00:00Z", false));
        assert_eq!(buf.auto.len(), 2);
        assert_eq!(buf.pinned.len(), 1);
        assert_eq!(buf.pinned[0].id, "p1");
    }

    #[test]
    fn step_back_walks_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().to_path_buf());
        let mut buf = TimelineBuffer::new(10);
        buf.push_auto(&store, make_cp("c1", "2026-01-01T00:00:00Z", false));
        buf.push_auto(&store, make_cp("c2", "2026-01-02T00:00:00Z", false));
        buf.push_auto(&store, make_cp("c3", "2026-01-03T00:00:00Z", false));
        // First step back => c2
        assert_eq!(buf.step_back().unwrap().id, "c2");
        // Next => c1
        assert_eq!(buf.step_back().unwrap().id, "c1");
        // No more
        assert!(buf.step_back().is_none());
        // Redo => c2
        assert_eq!(buf.step_forward().unwrap().id, "c2");
        // Redo => c3 (head)
        assert_eq!(buf.step_forward().unwrap().id, "c3");
    }

    #[test]
    fn reload_respects_capacity() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().to_path_buf());
        // Seed 5 auto + 2 pinned on disk.
        for i in 0..5 {
            let cp = make_cp(
                &format!("a{i}"),
                &format!("2026-01-0{}T00:00:00Z", i + 1),
                false,
            );
            store.save(&cp).unwrap();
        }
        for i in 0..2 {
            let cp = make_cp(
                &format!("p{i}"),
                &format!("2025-12-0{}T00:00:00Z", i + 1),
                true,
            );
            store.save(&cp).unwrap();
        }
        let mut buf = TimelineBuffer::new(3);
        buf.reload(&store);
        assert_eq!(buf.auto.len(), 3);
        assert_eq!(buf.pinned.len(), 2);

        // And the on-disk file count was pruned too.
        let remaining: usize = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(remaining, 5); // 3 auto + 2 pinned

        // Unused import suppression for RelativePosition
        let _ = RelativePosition {
            x_percent: 0.0,
            y_percent: 0.0,
        };
    }
}
