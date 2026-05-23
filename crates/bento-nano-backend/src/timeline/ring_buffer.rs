//! T-089 — in-memory index over the on-disk checkpoint store.
//!
//! Holds two lists:
//! - `auto`: bounded ring of auto-captured snapshots (default 20 slots).
//!   When full, the oldest entry is evicted from disk + memory.
//! - `pinned`: unbounded list of manual / permanent checkpoints.
//!
//! Mutated synchronously from the dispatcher; disk I/O happens inside
//! `push_auto` / `pin` / `remove` after the in-memory mutation.

use std::collections::VecDeque;

use super::checkpoint::{Checkpoint, CheckpointMeta, CheckpointStore};
use crate::time;
use smol_str::SmolStr;

/// Default auto-capture retention.
pub const DEFAULT_AUTO_CAPACITY: usize = 20;
/// Persistent coalescing horizon for selected-stack synchronous writers.
///
/// The async hook uses a 500 ms debounce plus a 2.5 s max burst window. The
/// disk-backed path only stores whole-second parseable `captured_at` values, so
/// 3 seconds is the closest safe horizon for "same burst" replacement.
pub const DEFAULT_PERSISTED_COALESCE_WINDOW_SECS: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoCoalesceMode {
    /// Keep the first checkpoint for this key. Used for pre-mutation anchors.
    KeepFirst,
    /// Replace the latest checkpoint for this key. Used for post-mutation head.
    ReplaceLatest,
}

pub struct TimelineBuffer {
    pub auto: VecDeque<Checkpoint>,
    pub pinned: Vec<Checkpoint>,
    pub auto_capacity: usize,
    /// Cursor into the merged sorted list used for undo/redo. `None`
    /// means "at head" (latest), otherwise the index inside the merged
    /// timeline.
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
        let (mut pinned, mut autos): (Vec<_>, Vec<_>) = all.drain(..).partition(|cp| cp.pinned);
        if autos.len() > self.auto_capacity {
            let drop_n = autos.len() - self.auto_capacity;
            for stale in autos.drain(..drop_n) {
                if let Err(e) = store.delete(&stale.id) {
                    tracing::warn!(
                        "Timeline: failed to evict stale auto checkpoint {}: {e}",
                        stale.id
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
            tracing::warn!("Timeline: failed to persist checkpoint {}: {e}", cp.id);
            return;
        }
        self.auto.push_back(cp);
        while self.auto.len() > self.auto_capacity {
            if let Some(stale) = self.auto.pop_front() {
                if let Err(e) = store.delete(&stale.id) {
                    tracing::warn!(
                        "Timeline: failed to evict auto checkpoint {}: {e}",
                        stale.id
                    );
                }
            }
        }
        self.cursor = None;
    }

    /// Push an auto checkpoint with durable burst coalescing.
    ///
    /// `KeepFirst` preserves the earliest checkpoint in a coalesced burst; this
    /// keeps undo/restore anchors honest. `ReplaceLatest` persists the incoming
    /// checkpoint, deletes the replaced JSON file, and updates memory so the
    /// visible head reflects the latest mutation without timeline spam.
    pub fn push_auto_coalesced(
        &mut self,
        store: &CheckpointStore,
        mut cp: Checkpoint,
        key: SmolStr,
        mode: AutoCoalesceMode,
    ) -> Checkpoint {
        cp.coalesce_key = Some(key.clone());
        let existing_pos = self
            .auto
            .iter()
            .rposition(|existing| can_coalesce(existing, &cp, key.as_str()));

        let Some(pos) = existing_pos else {
            self.push_auto(store, cp.clone());
            return cp;
        };

        match mode {
            AutoCoalesceMode::KeepFirst => {
                self.cursor = None;
                self.auto[pos].clone()
            }
            AutoCoalesceMode::ReplaceLatest => {
                let stale_id = self.auto[pos].id.clone();
                if let Err(e) = store.save(&cp) {
                    tracing::warn!(
                        "Timeline: failed to persist coalesced checkpoint {}: {e}",
                        cp.id
                    );
                    return self.auto[pos].clone();
                }
                if stale_id != cp.id {
                    if let Err(e) = store.delete(&stale_id) {
                        tracing::warn!(
                            "Timeline: failed to delete replaced coalesced checkpoint {stale_id}: {e}"
                        );
                    }
                }
                self.auto[pos] = cp.clone();
                self.cursor = None;
                cp
            }
        }
    }

    /// Pin a checkpoint (by id) so it is never auto-evicted. Returns the
    /// pinned checkpoint clone or `None` if `id` is not in the auto list.
    pub fn pin(&mut self, store: &CheckpointStore, id: &str) -> Option<Checkpoint> {
        let pos = self.auto.iter().position(|cp| cp.id.as_str() == id)?;
        let mut cp = self.auto.remove(pos)?;
        cp.pinned = true;
        if let Err(e) = store.save(&cp) {
            tracing::warn!("Timeline: failed to save pinned checkpoint: {e}");
        }
        self.pinned.push(cp.clone());
        self.pinned
            .sort_by(|a, b| a.snapshot.captured_at.cmp(&b.snapshot.captured_at));
        Some(cp)
    }

    /// Insert a newly-created pinned checkpoint (manual save).
    pub fn push_pinned(&mut self, store: &CheckpointStore, mut cp: Checkpoint) {
        cp.pinned = true;
        if let Err(e) = store.save(&cp) {
            tracing::warn!("Timeline: failed to save manual checkpoint: {e}");
            return;
        }
        self.pinned.push(cp);
        self.pinned
            .sort_by(|a, b| a.snapshot.captured_at.cmp(&b.snapshot.captured_at));
    }

    /// Remove a checkpoint from both memory and disk.
    pub fn remove(&mut self, store: &CheckpointStore, id: &str) -> bool {
        let removed_auto = if let Some(pos) = self.auto.iter().position(|cp| cp.id.as_str() == id) {
            self.auto.remove(pos).is_some()
        } else {
            false
        };
        let removed_pinned =
            if let Some(pos) = self.pinned.iter().position(|cp| cp.id.as_str() == id) {
                self.pinned.remove(pos);
                true
            } else {
                false
            };
        if removed_auto || removed_pinned {
            if let Err(e) = store.delete(id) {
                tracing::warn!("Timeline: failed to delete checkpoint {id}: {e}");
            }
            self.cursor = None;
            return true;
        }
        false
    }

    /// All checkpoints merged + sorted ascending by `captured_at`.
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
            let pos = merged.iter().position(|cp| cp.id.as_str() == id)?;
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

fn can_coalesce(existing: &Checkpoint, incoming: &Checkpoint, key: &str) -> bool {
    if existing.pinned || incoming.pinned {
        return false;
    }
    if existing.coalesce_key.as_deref() != Some(key) {
        return false;
    }
    if existing.trigger != incoming.trigger {
        return false;
    }
    let Ok(existing_secs) = time::parse_rfc3339_to_unix_secs(&existing.snapshot.captured_at) else {
        return false;
    };
    let Ok(incoming_secs) = time::parse_rfc3339_to_unix_secs(&incoming.snapshot.captured_at) else {
        return false;
    };
    let elapsed = incoming_secs.saturating_sub(existing_secs);
    (0..=DEFAULT_PERSISTED_COALESCE_WINDOW_SECS).contains(&elapsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{DesktopSnapshot, Resolution};
    use crate::timeline::checkpoint::DeltaSummary;
    use smol_str::SmolStr;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn scratch_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let tid = std::thread::current().id();
        let path = std::env::temp_dir().join(format!("bento-nano-rb-{tid:?}-{n}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn make_cp(id: &str, ts: &str, pinned: bool) -> Checkpoint {
        Checkpoint {
            id: SmolStr::from(id),
            snapshot: DesktopSnapshot {
                id: SmolStr::from(id),
                name: id.to_string(),
                resolution: Resolution {
                    width: 1920,
                    height: 1080,
                },
                dpi: 1.0,
                zones: Vec::new(),
                captured_at: SmolStr::from(ts),
            },
            delta: DeltaSummary::default(),
            delta_summary: String::new(),
            trigger: SmolStr::new_static("test"),
            coalesce_key: None,
            pinned,
        }
    }

    #[test]
    fn push_auto_evicts_oldest() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
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
        assert_eq!(buf.auto.front().expect("front").id.as_str(), "c2");
        assert_eq!(buf.auto.back().expect("back").id.as_str(), "c4");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pinned_are_never_evicted() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let mut buf = TimelineBuffer::new(2);
        buf.push_pinned(&store, make_cp("p1", "2026-01-01T00:00:00Z", false));
        buf.push_auto(&store, make_cp("a1", "2026-01-02T00:00:00Z", false));
        buf.push_auto(&store, make_cp("a2", "2026-01-03T00:00:00Z", false));
        buf.push_auto(&store, make_cp("a3", "2026-01-04T00:00:00Z", false));
        assert_eq!(buf.auto.len(), 2);
        assert_eq!(buf.pinned.len(), 1);
        assert_eq!(buf.pinned[0].id.as_str(), "p1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn step_back_walks_cursor() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let mut buf = TimelineBuffer::new(10);
        buf.push_auto(&store, make_cp("c1", "2026-01-01T00:00:00Z", false));
        buf.push_auto(&store, make_cp("c2", "2026-01-02T00:00:00Z", false));
        buf.push_auto(&store, make_cp("c3", "2026-01-03T00:00:00Z", false));
        assert_eq!(buf.step_back().expect("c2").id.as_str(), "c2");
        assert_eq!(buf.step_back().expect("c1").id.as_str(), "c1");
        assert!(buf.step_back().is_none());
        assert_eq!(buf.step_forward().expect("c2").id.as_str(), "c2");
        assert_eq!(buf.step_forward().expect("c3").id.as_str(), "c3");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reload_respects_capacity() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        for i in 0..5 {
            let cp = make_cp(
                &format!("a{i}"),
                &format!("2026-01-0{}T00:00:00Z", i + 1),
                false,
            );
            store.save(&cp).expect("seed auto");
        }
        for i in 0..2 {
            let cp = make_cp(
                &format!("p{i}"),
                &format!("2025-12-0{}T00:00:00Z", i + 1),
                true,
            );
            store.save(&cp).expect("seed pinned");
        }
        let mut buf = TimelineBuffer::new(3);
        buf.reload(&store);
        assert_eq!(buf.auto.len(), 3);
        assert_eq!(buf.pinned.len(), 2);

        let remaining: usize = std::fs::read_dir(&dir).expect("readdir").count();
        assert_eq!(remaining, 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pin_promotes_auto_to_pinned() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let mut buf = TimelineBuffer::new(3);
        buf.push_auto(&store, make_cp("c1", "2026-01-01T00:00:00Z", false));
        let pinned = buf.pin(&store, "c1").expect("pin");
        assert!(pinned.pinned);
        assert!(buf.auto.is_empty());
        assert_eq!(buf.pinned.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_clears_disk_and_memory() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let mut buf = TimelineBuffer::new(3);
        buf.push_auto(&store, make_cp("c1", "2026-01-01T00:00:00Z", false));
        assert!(buf.remove(&store, "c1"));
        assert!(buf.auto.is_empty());
        assert!(store.load("c1").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn push_auto_coalesced_keep_first_preserves_anchor() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let mut buf = TimelineBuffer::new(3);

        let first = buf.push_auto_coalesced(
            &store,
            make_cp("pre-1", "2026-01-01T00:00:00Z", false),
            SmolStr::new_static("rule:pre"),
            AutoCoalesceMode::KeepFirst,
        );
        let second = buf.push_auto_coalesced(
            &store,
            make_cp("pre-2", "2026-01-01T00:00:01Z", false),
            SmolStr::new_static("rule:pre"),
            AutoCoalesceMode::KeepFirst,
        );

        assert_eq!(first.id.as_str(), "pre-1");
        assert_eq!(second.id.as_str(), "pre-1");
        assert_eq!(buf.auto.len(), 1);
        assert!(store.load("pre-1").is_ok());
        assert!(store.load("pre-2").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn push_auto_coalesced_replace_latest_rewrites_disk_and_memory() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let mut buf = TimelineBuffer::new(3);

        buf.push_auto_coalesced(
            &store,
            make_cp("post-1", "2026-01-01T00:00:00Z", false),
            SmolStr::new_static("rule:post"),
            AutoCoalesceMode::ReplaceLatest,
        );
        let latest = buf.push_auto_coalesced(
            &store,
            make_cp("post-2", "2026-01-01T00:00:01Z", false),
            SmolStr::new_static("rule:post"),
            AutoCoalesceMode::ReplaceLatest,
        );

        assert_eq!(latest.id.as_str(), "post-2");
        assert_eq!(buf.auto.len(), 1);
        assert_eq!(buf.auto[0].id.as_str(), "post-2");
        assert!(store.load("post-1").is_err());
        assert!(store.load("post-2").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn push_auto_coalesced_keeps_distinct_keys_and_old_windows() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let mut buf = TimelineBuffer::new(10);

        buf.push_auto_coalesced(
            &store,
            make_cp("a", "2026-01-01T00:00:00Z", false),
            SmolStr::new_static("rule:a"),
            AutoCoalesceMode::ReplaceLatest,
        );
        buf.push_auto_coalesced(
            &store,
            make_cp("b", "2026-01-01T00:00:01Z", false),
            SmolStr::new_static("rule:b"),
            AutoCoalesceMode::ReplaceLatest,
        );
        buf.push_auto_coalesced(
            &store,
            make_cp("a-late", "2026-01-01T00:00:10Z", false),
            SmolStr::new_static("rule:a"),
            AutoCoalesceMode::ReplaceLatest,
        );

        assert_eq!(buf.auto.len(), 3);
        assert!(store.load("a").is_ok());
        assert!(store.load("b").is_ok());
        assert!(store.load("a-late").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn seek_to_head_clears_cursor() {
        let dir = scratch_dir();
        let store = CheckpointStore::new(dir.clone());
        let mut buf = TimelineBuffer::new(10);
        buf.push_auto(&store, make_cp("c1", "2026-01-01T00:00:00Z", false));
        buf.push_auto(&store, make_cp("c2", "2026-01-02T00:00:00Z", false));
        let target = buf.seek("c2").expect("seek");
        assert_eq!(target.id.as_str(), "c2");
        assert!(buf.cursor.is_none()); // c2 is the head
        let _ = std::fs::remove_dir_all(&dir);
    }
}
