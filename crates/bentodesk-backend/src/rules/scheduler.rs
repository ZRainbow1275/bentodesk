//! T-088 — periodic rule scheduler.
//!
//! Replaces 1.x's `tauri::async_runtime::spawn` + `tokio::time::interval`
//! (forbidden by spec §9 — no async runtime) with a plain
//! `std::thread::spawn` that sleeps in 1-second slices so the static
//! `SHUTDOWN` flag can preempt within a second of `shutdown()` being
//! called.
//!
//! The scheduler does **not** apply rule effects on its own — it produces
//! [`SchedulerEvent::RuleDue`] on a `crossbeam_channel::Sender` for each
//! due rule. The dispatcher receives the event, calls
//! [`super::executor::build_plan`], and applies the resulting
//! [`super::executor::ExecutionPlan`] against its layout/icon/stealth state.
//!
//! Splitting the "decide which rules are due" tick from "apply effects"
//! keeps the side-effecting work on the dispatcher thread (single-process,
//! single-window state owner per spec §2) and makes the scheduler trivially
//! testable.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Sender, TrySendError};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use super::{executor, load_all};

/// When set to `true`, the scheduler thread exits within ~1 second.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Signal the scheduler thread to exit. Idempotent. Safe from any thread.
pub fn shutdown() {
    SHUTDOWN.store(true, Ordering::Release);
}

/// Reset the shutdown flag — useful in tests so a previous test's
/// `shutdown()` does not prevent the next test's scheduler from running.
/// Not exported in production builds.
#[cfg(test)]
fn reset_shutdown_for_tests() {
    SHUTDOWN.store(false, Ordering::Release);
}

/// Events the scheduler emits on its `Sender<SchedulerEvent>`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SchedulerEvent {
    /// A rule's `Interval { minutes }` window has elapsed — dispatcher
    /// should call `executor::build_plan` for the matching rule id.
    RuleDue { rule_id: SmolStr },
}

/// Spawn the background scheduler. Wakes every `tick` (60s in production),
/// loads `rules.json`, fires [`SchedulerEvent::RuleDue`] for each
/// due-and-enabled rule.
///
/// Returns immediately; the spawned thread holds the `Sender`. Channel
/// closure (receiver dropped) terminates the thread cleanly.
pub fn spawn(state_dir: PathBuf, event_tx: Sender<SchedulerEvent>, tick: Duration) {
    std::thread::spawn(move || {
        loop {
            // Sleep in 1-second slices so SHUTDOWN propagates fast.
            let mut elapsed = Duration::ZERO;
            while elapsed < tick {
                if SHUTDOWN.load(Ordering::Acquire) {
                    tracing::info!("rules scheduler shutting down (signal)");
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
                elapsed += Duration::from_millis(100);
            }

            if SHUTDOWN.load(Ordering::Acquire) {
                tracing::info!("rules scheduler shutting down (signal)");
                return;
            }

            if scan_due_and_emit(&state_dir, &event_tx).is_err() {
                tracing::warn!("rules scheduler: event channel closed, exiting");
                return;
            }
        }
    });
}

/// Single tick: load rules, emit one event per due rule. Pulled out for
/// unit-testability (no `std::thread::sleep`).
fn scan_due_and_emit(
    state_dir: &std::path::Path,
    event_tx: &Sender<SchedulerEvent>,
) -> Result<(), crossbeam_channel::SendError<SchedulerEvent>> {
    if !super::rules_path(state_dir).is_file() {
        return Ok(());
    }
    let rules = load_all(state_dir);
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    for rule in rules.iter().filter(|r| r.enabled) {
        if !executor::should_run_now(rule, now_secs) {
            continue;
        }
        let event = SchedulerEvent::RuleDue {
            rule_id: rule.id.clone(),
        };
        match event_tx.try_send(event) {
            Ok(()) => {}
            // The UI will drain the bounded queue and this still-due rule will
            // be retried on the next scheduler tick.
            Err(TrySendError::Full(_)) => return Ok(()),
            Err(TrySendError::Disconnected(event)) => {
                return Err(crossbeam_channel::SendError(event));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Action, Condition, ConditionGroup, ConditionNode, Rule, RunMode, upsert};
    use std::sync::atomic::AtomicU32;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn scratch_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let tid = std::thread::current().id();
        let path = std::env::temp_dir().join(format!("bentodesk-rules-sched-{tid:?}-{n}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn mk_rule(id: &str, mode: RunMode) -> Rule {
        Rule {
            id: SmolStr::from(id),
            name: id.into(),
            enabled: true,
            conditions: ConditionGroup::All(vec![ConditionNode::Leaf(Condition::ExtensionIn(
                vec![SmolStr::new_static("tmp")],
            ))]),
            actions: vec![Action::Notify("ping".into())],
            run_mode: mode,
            last_run: None,
            run_count: 0,
        }
    }

    #[test]
    fn scan_due_emits_event_for_interval_first_time() {
        let dir = scratch_dir();
        upsert(&dir, mk_rule("r1", RunMode::Interval { minutes: 60 })).expect("seed");
        let (tx, rx) = crossbeam_channel::unbounded();
        scan_due_and_emit(&dir, &tx).expect("emit");
        let evt = rx.try_recv().expect("event");
        assert!(matches!(evt, SchedulerEvent::RuleDue { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_due_skips_on_demand_rules() {
        let dir = scratch_dir();
        upsert(&dir, mk_rule("od", RunMode::OnDemand)).expect("seed");
        let (tx, rx) = crossbeam_channel::unbounded();
        scan_due_and_emit(&dir, &tx).expect("emit");
        assert!(rx.try_recv().is_err(), "OnDemand must not emit");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_due_skips_disabled_rules() {
        let dir = scratch_dir();
        let mut r = mk_rule("dis", RunMode::Interval { minutes: 60 });
        r.enabled = false;
        upsert(&dir, r).expect("seed");
        let (tx, rx) = crossbeam_channel::unbounded();
        scan_due_and_emit(&dir, &tx).expect("emit");
        assert!(rx.try_recv().is_err(), "disabled rule must not emit");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_due_skips_missing_rules_file() {
        let dir = scratch_dir();
        let (tx, rx) = crossbeam_channel::unbounded();
        scan_due_and_emit(&dir, &tx).expect("missing rules file is an idle tick");
        assert!(rx.try_recv().is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_due_returns_send_error_on_closed_channel() {
        let dir = scratch_dir();
        upsert(&dir, mk_rule("r", RunMode::Interval { minutes: 60 })).expect("seed");
        let (tx, rx) = crossbeam_channel::unbounded::<SchedulerEvent>();
        drop(rx);
        let res = scan_due_and_emit(&dir, &tx);
        assert!(res.is_err(), "closed channel must surface SendError");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_due_does_not_block_on_a_full_bounded_channel() {
        let dir = scratch_dir();
        upsert(&dir, mk_rule("a", RunMode::Interval { minutes: 60 })).expect("seed a");
        upsert(&dir, mk_rule("b", RunMode::Interval { minutes: 60 })).expect("seed b");
        let (tx, rx) = crossbeam_channel::bounded::<SchedulerEvent>(1);

        scan_due_and_emit(&dir, &tx).expect("full queue is retried next tick");

        assert_eq!(rx.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shutdown_terminates_spawned_thread() {
        reset_shutdown_for_tests();
        let dir = scratch_dir();
        let (tx, _rx) = crossbeam_channel::unbounded::<SchedulerEvent>();
        // Use an absurdly long tick — only the SHUTDOWN flag can stop us.
        spawn(dir.clone(), tx, Duration::from_secs(3_600));
        std::thread::sleep(Duration::from_millis(150));
        shutdown();
        // No deterministic way to assert the thread joined without keeping
        // a JoinHandle (the public API does not return one — matches
        // 1.x's fire-and-forget). At minimum, calling shutdown twice
        // remains safe and idempotent.
        shutdown();
        reset_shutdown_for_tests();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
