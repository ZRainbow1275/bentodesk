//! T-089 — write-hook surface for the timeline.
//!
//! Dispatcher modules call [`TimelineHook::record_change`] AFTER a
//! successful mutation. The call is non-blocking: it updates the shared
//! debounce state and starts (or re-arms) a 500 ms timer thread. When the
//! timer fires without further activity, a single checkpoint is captured
//! for the entire burst.
//!
//! ## Why debounce?
//! Bulk operations (auto-grouping 20 files, reordering items) would
//! otherwise produce 20 checkpoints and instantly push real history out of
//! the ring buffer. Coalescing into one checkpoint per 500 ms window keeps
//! the timeline useful without slowing down the dispatcher.
//!
//! ## What changed vs 1.x
//!
//! The 1.x hook reached into `AppState` (for layout) and `AppHandle` (for
//! `emit("timeline_updated", id)`). Both are gone:
//!
//! 1. The dispatcher injects a [`SnapshotProvider`] closure at construction
//!    time. The hook calls it on every `record_change` and on every flush
//!    to obtain the current `DesktopSnapshot` — same contract as 1.x's
//!    `capture_current_snapshot`, but without the global state reach.
//! 2. The hook owns a `crossbeam_channel::Sender<TimelineEvent>` instead
//!    of `app.emit`. The dispatcher receives `TimelineUpdated { id }` and
//!    routes it to its own UI bus.
//! 3. The hook owns its own `Arc<Mutex<TimelineBuffer>>` and the
//!    `CheckpointStore` directory. There is no dispatcher-side lock dance
//!    inside `flush_checkpoint` (1.x had to drop the timeline mutex
//!    before touching the hook-state mutex; the native hook simply never
//!    holds two locks together).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use super::checkpoint::{
    Checkpoint, CheckpointStore, DeltaSummary, compute_delta, new_checkpoint_id,
};
use super::ring_buffer::TimelineBuffer;
use crate::layout::DesktopSnapshot;

/// Default base debounce window — most bursts coalesce within 500 ms.
pub const DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);

/// Upper bound on how long a coalesced window can grow. Pathological
/// re-arms (layout algorithms hammering 200 zones) still flush within
/// 2.5 s.
pub const COALESCE_MAX_WINDOW: Duration = Duration::from_millis(2_500);

/// Threshold used by [`on_significant_change`] — below this the coalesced
/// checkpoint is skipped to avoid noise.
pub const SIGNIFICANT_ITEM_THRESHOLD: i32 = 3;

/// Pluggable snapshot capture. The dispatcher injects a closure that
/// reads its live `LayoutData` + queries the screen resolution; the hook
/// invokes it on every record + every flush.
pub type SnapshotProvider = Arc<dyn Fn() -> DesktopSnapshot + Send + Sync>;

/// Events the hook emits on its `Sender<TimelineEvent>`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TimelineEvent {
    /// A new auto checkpoint was committed; the dispatcher should refresh
    /// the timeline UI.
    Updated { checkpoint_id: SmolStr },
}

#[derive(Default)]
struct HookState {
    /// The last fully-committed snapshot (used as the diff baseline).
    baseline: Option<DesktopSnapshot>,
    /// Accumulated delta across coalesced triggers.
    pending_delta: DeltaSummary,
    /// The last trigger name (most recent wins when multiple fire in the
    /// window).
    pending_trigger: SmolStr,
    /// When the current debounce window started.
    window_started: Option<Instant>,
    /// Earliest timestamp this window saw activity — used by
    /// [`COALESCE_MAX_WINDOW`] to bound coalesced-burst duration.
    window_opened_at: Option<Instant>,
    /// `true` when a background timer is currently armed.
    timer_running: bool,
}

/// The timeline write-hook. Owns its debounce state, snapshot provider,
/// store, and a shared reference to the dispatcher's [`TimelineBuffer`].
pub struct TimelineHook {
    state: Arc<Mutex<HookState>>,
    snapshot_provider: SnapshotProvider,
    buffer: Arc<Mutex<TimelineBuffer>>,
    store_dir: PathBuf,
    event_tx: Sender<TimelineEvent>,
    debounce_window: Duration,
    coalesce_max_window: Duration,
}

impl TimelineHook {
    /// Construct a hook. `store_dir` should be the
    /// `<state_dir>/timeline` directory; the hook never escapes it.
    pub fn new(
        snapshot_provider: SnapshotProvider,
        buffer: Arc<Mutex<TimelineBuffer>>,
        store_dir: PathBuf,
        event_tx: Sender<TimelineEvent>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(Mutex::new(HookState::default())),
            snapshot_provider,
            buffer,
            store_dir,
            event_tx,
            debounce_window: DEBOUNCE_WINDOW,
            coalesce_max_window: COALESCE_MAX_WINDOW,
        })
    }

    /// Test-only constructor with custom debounce timings so tests can run
    /// in milliseconds rather than seconds.
    #[cfg(test)]
    fn new_with_windows(
        snapshot_provider: SnapshotProvider,
        buffer: Arc<Mutex<TimelineBuffer>>,
        store_dir: PathBuf,
        event_tx: Sender<TimelineEvent>,
        debounce_window: Duration,
        coalesce_max_window: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(Mutex::new(HookState::default())),
            snapshot_provider,
            buffer,
            store_dir,
            event_tx,
            debounce_window,
            coalesce_max_window,
        })
    }

    /// Prime the baseline — call once at startup after layout is loaded.
    pub fn init_baseline(&self) {
        let snap = (self.snapshot_provider)();
        // Recover from poisoning — the lock guards a small struct, no
        // invariants depend on the prior panic.
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.baseline = Some(snap);
    }

    /// Register a mutation with the timeline. Cheap to call from any
    /// dispatcher write path: it mostly takes a snapshot + locks the
    /// debounce state. Persistence happens asynchronously after the
    /// debounce window elapses.
    pub fn record_change(self: &Arc<Self>, trigger: &str) {
        let snap = (self.snapshot_provider)();
        let now = Instant::now();
        let need_spawn = {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            // Recompute the merged delta against the unchanged baseline —
            // simpler than incremental merging and always correct.
            s.pending_delta = compute_delta(s.baseline.as_ref(), &snap.zones);
            s.pending_trigger = SmolStr::from(trigger);
            s.window_started = Some(now);
            if s.window_opened_at.is_none() {
                s.window_opened_at = Some(now);
            }
            if s.timer_running {
                false
            } else {
                s.timer_running = true;
                true
            }
        };

        if need_spawn {
            let me = Arc::clone(self);
            std::thread::spawn(move || me.debounce_loop());
        }
    }

    fn debounce_loop(self: Arc<Self>) {
        loop {
            std::thread::sleep(self.debounce_window);
            let decision = {
                let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                let window_open_long_enough = s
                    .window_opened_at
                    .is_some_and(|t| t.elapsed() >= self.coalesce_max_window);
                match s.window_started {
                    Some(started)
                        if started.elapsed() >= self.debounce_window || window_open_long_enough =>
                    {
                        s.window_started = None;
                        s.window_opened_at = None;
                        s.timer_running = false;
                        Some((
                            std::mem::take(&mut s.pending_delta),
                            std::mem::take(&mut s.pending_trigger),
                        ))
                    }
                    Some(_) => None,
                    None => {
                        s.timer_running = false;
                        return;
                    }
                }
            };

            if let Some((delta, trigger)) = decision {
                self.flush_checkpoint(delta, &trigger);
                return;
            }
        }
    }

    fn flush_checkpoint(&self, delta: DeltaSummary, trigger: &str) {
        if !on_significant_change(&delta) {
            tracing::debug!(
                "Timeline: skip non-significant checkpoint (items={}, zones={}, trigger={trigger})",
                delta.item_churn(),
                delta.zone_churn()
            );
            return;
        }

        let snap = (self.snapshot_provider)();
        let summary = delta.human();
        let cp = Checkpoint {
            id: new_checkpoint_id(),
            snapshot: snap.clone(),
            delta: delta.clone(),
            delta_summary: summary,
            trigger: SmolStr::from(trigger),
            coalesce_key: Some(SmolStr::from(format!("hook:{trigger}"))),
            pinned: false,
        };
        let cp_id = cp.id.clone();

        let store = CheckpointStore::new(self.store_dir.clone());
        // Hold the buffer lock briefly; never grab `self.state` while
        // holding it (deadlock-avoidance discipline preserved from 1.x).
        {
            let mut buf = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
            buf.push_auto(&store, cp);
        }

        // Update the baseline so the next window diffs against this point.
        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.baseline = Some(snap);
        }

        let _ = self.event_tx.send(TimelineEvent::Updated {
            checkpoint_id: cp_id,
        });
    }
}

/// Determines whether a coalesced delta deserves a checkpoint entry.
/// Pure + deterministic — kept separate from the hook so it can be
/// unit-tested without any I/O.
pub fn on_significant_change(delta: &DeltaSummary) -> bool {
    delta.item_churn() >= SIGNIFICANT_ITEM_THRESHOLD || delta.zone_churn() >= 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{
        BentoItem, BentoZone, GridPosition, ItemType, RelativePosition, RelativeSize, Resolution,
    };
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn scratch_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let tid = std::thread::current().id();
        let path = std::env::temp_dir().join(format!("bentodesk-hook-{tid:?}-{n}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn empty_snapshot() -> DesktopSnapshot {
        DesktopSnapshot {
            id: SmolStr::new_static("s"),
            name: String::new(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
            zones: Vec::new(),
            captured_at: SmolStr::new_static("2026-01-01T00:00:00Z"),
        }
    }

    fn snapshot_with_n_zones(n: usize) -> DesktopSnapshot {
        let zones = (0..n)
            .map(|i| BentoZone {
                id: SmolStr::from(format!("z{i}")),
                name: format!("Z{i}"),
                icon: SmolStr::new_static("f"),
                position: RelativePosition {
                    x_percent: 0.0,
                    y_percent: 0.0,
                },
                expanded_size: RelativeSize {
                    w_percent: 30.0,
                    h_percent: 30.0,
                },
                items: vec![BentoItem {
                    id: SmolStr::from(format!("i-{i}")),
                    zone_id: SmolStr::from(format!("z{i}")),
                    item_type: ItemType::File,
                    name: format!("item-{i}"),
                    path: format!("C:/x{i}"),
                    icon_hash: SmolStr::new_static(""),
                    grid_position: GridPosition {
                        col: 0,
                        row: 0,
                        col_span: 1,
                    },
                    is_wide: false,
                    added_at: SmolStr::new_static(""),
                    original_path: None,
                    hidden_path: None,
                    icon_x: None,
                    icon_y: None,
                    file_missing: false,
                    tags: Vec::new(),
                }],
                accent_color: None,
                sort_order: 0,
                auto_group: None,
                grid_columns: 4,
                created_at: SmolStr::new_static(""),
                updated_at: SmolStr::new_static(""),
                capsule_size: SmolStr::new_static("medium"),
                capsule_shape: SmolStr::new_static("pill"),
                locked: false,
                visible: true,
                stack_id: None,
                stack_order: 0,
                alias: None,
                display_mode: None,
                live_folder_path: None,
            })
            .collect();
        DesktopSnapshot {
            id: SmolStr::new_static("s"),
            name: String::new(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
            zones,
            captured_at: SmolStr::new_static("2026-01-02T00:00:00Z"),
        }
    }

    // ─── Pure threshold tests ────────────────────────────────────

    #[test]
    fn threshold_requires_three_item_changes() {
        let d = DeltaSummary {
            items_added: 2,
            ..Default::default()
        };
        assert!(!on_significant_change(&d));
        let d = DeltaSummary {
            items_added: 3,
            ..Default::default()
        };
        assert!(on_significant_change(&d));
    }

    #[test]
    fn any_zone_change_is_significant() {
        let d = DeltaSummary {
            zones_added: 1,
            ..Default::default()
        };
        assert!(on_significant_change(&d));
        let d = DeltaSummary {
            zones_removed: 1,
            ..Default::default()
        };
        assert!(on_significant_change(&d));
    }

    #[test]
    fn trivial_change_is_skipped() {
        let d = DeltaSummary::default();
        assert!(!on_significant_change(&d));
        let d = DeltaSummary {
            items_moved: 1,
            ..Default::default()
        };
        assert!(!on_significant_change(&d));
    }

    #[test]
    fn coalesce_max_window_bounds_bursts() {
        assert!(
            COALESCE_MAX_WINDOW >= DEBOUNCE_WINDOW,
            "coalesce cap must exceed debounce window"
        );
        assert!(
            COALESCE_MAX_WINDOW.as_millis() <= 5_000,
            "coalesce cap must stay tight enough for UX"
        );
    }

    // ─── End-to-end debounce + flush ─────────────────────────────

    #[test]
    fn record_change_flushes_significant_checkpoint() {
        let dir = scratch_dir();
        let buffer = Arc::new(Mutex::new(TimelineBuffer::new(5)));
        let (tx, rx) = crossbeam_channel::unbounded();

        // Snapshot provider returns 3 zones — which is a 3-zone-add delta
        // against the empty baseline (significant).
        let provider: SnapshotProvider = Arc::new(snapshot_with_3_zones);

        let hook = TimelineHook::new_with_windows(
            provider,
            Arc::clone(&buffer),
            dir.clone(),
            tx,
            Duration::from_millis(50),
            Duration::from_millis(200),
        );
        hook.init_baseline_with_snapshot(empty_snapshot());

        hook.record_change("test_burst");

        // Wait for the debounce window + a small margin.
        let evt = rx
            .recv_timeout(Duration::from_millis(500))
            .expect("checkpoint event must fire");
        assert!(matches!(evt, TimelineEvent::Updated { .. }));

        let buf = buffer.lock().expect("buf");
        assert_eq!(buf.auto.len(), 1);
        assert_eq!(buf.auto[0].trigger.as_str(), "test_burst");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_change_skips_non_significant_delta() {
        let dir = scratch_dir();
        let buffer = Arc::new(Mutex::new(TimelineBuffer::new(5)));
        let (tx, rx) = crossbeam_channel::unbounded();

        // Empty snapshot ⇒ 0-zone, 0-item delta against empty baseline ⇒
        // not significant ⇒ flush is a no-op.
        let provider: SnapshotProvider = Arc::new(empty_snapshot);

        let hook = TimelineHook::new_with_windows(
            provider,
            Arc::clone(&buffer),
            dir.clone(),
            tx,
            Duration::from_millis(50),
            Duration::from_millis(200),
        );
        hook.init_baseline_with_snapshot(empty_snapshot());

        hook.record_change("trivial");
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        assert!(buffer.lock().expect("buf").auto.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn snapshot_with_3_zones() -> DesktopSnapshot {
        snapshot_with_n_zones(3)
    }

    impl TimelineHook {
        /// Test-only baseline injection that doesn't go through the
        /// snapshot provider (so `init_baseline` calls in tests aren't
        /// counted twice when the provider counts invocations).
        #[cfg(test)]
        fn init_baseline_with_snapshot(&self, snap: DesktopSnapshot) {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.baseline = Some(snap);
        }
    }
}
