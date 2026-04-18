//! Write-hook surface for the timeline.
//!
//! Command modules call [`record_change`] AFTER a successful mutation. This
//! function is non-blocking: it updates a shared "pending" state and starts (or
//! re-arms) a 500 ms timer thread. When the timer fires without further
//! activity, a single checkpoint is captured for the entire burst.
//!
//! ## Why debounce?
//! Bulk operations (auto-grouping 20 files, reordering items) would otherwise
//! produce 20 checkpoints and instantly push real history out of the ring
//! buffer. Coalescing into one checkpoint per 500 ms window keeps the timeline
//! useful without slowing down the UI.
//!
//! ## `on_significant_change` threshold
//! Auto checkpoints are only taken when the *coalesced* delta reaches at least
//! 3 item-level changes OR any zone-level change. Manual saves bypass this.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use super::checkpoint::{self, compute_delta, Checkpoint, CheckpointStore, DeltaSummary};

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);
/// Upper bound on how long a coalesced window can grow. Bulk operations
/// (layout algorithms applied to 200 zones) can keep re-arming the debounce
/// for longer than the base 500 ms; without a hard ceiling they would never
/// flush. Once the window age exceeds this, we flush regardless of activity.
const COALESCE_MAX_WINDOW: Duration = Duration::from_millis(2_500);
/// Threshold used by `on_significant_change` — below this, the coalesced
/// checkpoint is skipped to avoid noise.
pub const SIGNIFICANT_ITEM_THRESHOLD: i32 = 3;

/// Shared mutable state across all hook callers within one process.
#[derive(Default)]
struct HookState {
    /// The last fully-committed snapshot (used as the diff baseline).
    baseline: Option<crate::layout::snapshot::DesktopSnapshot>,
    /// Accumulated delta across coalesced triggers.
    pending_delta: DeltaSummary,
    /// The last trigger name (most recent wins when multiple fire in the window).
    pending_trigger: String,
    /// When the current debounce window started.
    window_started: Option<Instant>,
    /// Earliest timestamp this window saw activity — used by
    /// `COALESCE_MAX_WINDOW` to bound coalesced-burst duration.
    window_opened_at: Option<Instant>,
    /// `true` when a background timer is currently armed.
    timer_running: bool,
    /// When `true`, the debounce timer allows `window_started` to re-arm
    /// within a single checkpoint; used by bulk operations whose duration
    /// can exceed the base 500 ms window.
    coalesce_in_progress: bool,
}

fn state() -> &'static Mutex<HookState> {
    static STATE: OnceLock<Mutex<HookState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(HookState::default()))
}

/// Compute the timeline storage directory: `%APPDATA%/BentoDesk/timeline/`.
pub fn timeline_dir(app: &AppHandle) -> std::path::PathBuf {
    match app.path().app_data_dir() {
        Ok(p) => p.join("timeline"),
        Err(_) => std::path::PathBuf::from(".")
            .join("BentoDesk")
            .join("timeline"),
    }
}

fn capture_current_snapshot(app: &AppHandle) -> crate::layout::snapshot::DesktopSnapshot {
    let state = app.state::<crate::AppState>();
    let layout = match state.layout.lock() {
        Ok(l) => l.clone(),
        Err(e) => {
            tracing::error!("Timeline: layout lock poisoned: {e}");
            crate::layout::persistence::LayoutData::default()
        }
    };
    let res = crate::layout::resolution::get_current_resolution();
    let dpi = crate::layout::resolution::get_dpi_scale();
    crate::layout::snapshot::DesktopSnapshot {
        id: uuid::Uuid::new_v4().to_string(),
        name: String::new(),
        resolution: res,
        dpi,
        zones: layout.zones,
        captured_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Prime the baseline — called once at startup after layout is loaded.
pub fn init_baseline(app: &AppHandle) {
    let snap = capture_current_snapshot(app);
    if let Ok(mut s) = state().lock() {
        s.baseline = Some(snap);
    }
}

/// Frontend-facing wrapper: register a mutation with the timeline.
///
/// This is cheap enough to call from every `#[tauri::command]` without
/// noticeable overhead (lock + clone of current zones). The actual checkpoint
/// is persisted asynchronously once the debounce window elapses.
pub fn record_change(app: &AppHandle, trigger: &str) {
    record_internal(app, trigger, false);
}

/// Bulk-mode variant of [`record_change`].
///
/// Sets the sliding `coalesce_in_progress` flag so concurrent single-zone
/// updates triggered by the same bulk IPC don't each produce their own
/// checkpoint — they all fold into a single delta against the pre-bulk
/// baseline. Bounded by [`COALESCE_MAX_WINDOW`] so pathological bursts
/// still flush.
pub fn record_change_coalesced(app: &AppHandle, trigger: &str) {
    record_internal(app, trigger, true);
}

fn record_internal(app: &AppHandle, trigger: &str, coalesce: bool) {
    let snap = capture_current_snapshot(app);
    let now = Instant::now();
    let mut s = match state().lock() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Timeline hook lock poisoned: {e}");
            return;
        }
    };
    let delta = compute_delta(s.baseline.as_ref(), &snap.zones);
    // Keep a running "merged" delta. Easiest correct thing: recompute against
    // the baseline (which hasn't moved yet during the debounce window).
    s.pending_delta = delta;
    s.pending_trigger = trigger.to_string();
    s.window_started = Some(now);
    if s.window_opened_at.is_none() {
        s.window_opened_at = Some(now);
    }
    if coalesce {
        s.coalesce_in_progress = true;
    }

    if !s.timer_running {
        s.timer_running = true;
        drop(s); // release before spawning
        let handle = app.clone();
        std::thread::spawn(move || debounce_loop(handle));
    }
}

fn debounce_loop(app: AppHandle) {
    loop {
        std::thread::sleep(DEBOUNCE_WINDOW);
        let decision = {
            let mut s = match state().lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            let window_open_long_enough = s
                .window_opened_at
                .is_some_and(|t| t.elapsed() >= COALESCE_MAX_WINDOW);
            match s.window_started {
                Some(started)
                    if started.elapsed() >= DEBOUNCE_WINDOW || window_open_long_enough =>
                {
                    // Window closed — flush.
                    s.window_started = None;
                    s.window_opened_at = None;
                    s.coalesce_in_progress = false;
                    s.timer_running = false;
                    Some((
                        std::mem::take(&mut s.pending_delta),
                        std::mem::take(&mut s.pending_trigger),
                    ))
                }
                Some(_) => None, // re-armed, keep looping
                None => {
                    s.timer_running = false;
                    return;
                }
            }
        };

        if let Some((delta, trigger)) = decision {
            flush_checkpoint(&app, delta, &trigger);
            return;
        }
    }
}

fn flush_checkpoint(app: &AppHandle, delta: DeltaSummary, trigger: &str) {
    // Suppress trivial changes below the threshold.
    if !on_significant_change(&delta) {
        tracing::debug!(
            "Timeline: skip non-significant checkpoint (items={}, zones={}, trigger={trigger})",
            delta.item_churn(),
            delta.zone_churn()
        );
        return;
    }

    let snap = capture_current_snapshot(app);
    let summary = delta.human();
    let cp = Checkpoint {
        id: checkpoint::new_checkpoint_id(),
        snapshot: snap.clone(),
        delta: delta.clone(),
        delta_summary: summary,
        trigger: trigger.to_string(),
        pinned: false,
    };
    let cp_id = cp.id.clone();

    let store = CheckpointStore::new(timeline_dir(app));
    let app_state = app.state::<crate::AppState>();
    let inner = match app_state.timeline.lock() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Timeline buffer lock poisoned: {e}");
            return;
        }
    };
    let mut buf = inner;
    buf.push_auto(&store, cp);
    // Release the timeline mutex BEFORE touching the hook-state mutex, so
    // concurrent IPCs (list_checkpoints / undo) don't stall behind our disk
    // write and the two locks never need to be held together.
    drop(buf);

    // Update the baseline to the new snapshot so the *next* window diffs
    // against this point, not against the pre-burst state.
    if let Ok(mut s) = state().lock() {
        s.baseline = Some(snap);
    }

    // Notify frontend so the timeline UI can refresh.
    let _ = app.emit("timeline_updated", cp_id);
}

/// Determines whether a coalesced delta deserves a checkpoint entry.
/// Kept small & deterministic so it can be unit-tested.
pub fn on_significant_change(delta: &DeltaSummary) -> bool {
    delta.item_churn() >= SIGNIFICANT_ITEM_THRESHOLD || delta.zone_churn() >= 1
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Sanity check the bound itself — prevents silent drift.
        assert!(
            COALESCE_MAX_WINDOW >= DEBOUNCE_WINDOW,
            "coalesce cap must exceed debounce window"
        );
        assert!(
            COALESCE_MAX_WINDOW.as_millis() <= 5_000,
            "coalesce cap must stay tight enough for UX"
        );
    }
}
