//! T-096 — power-resume coordination.
//!
//! When a laptop sleeps or hibernates, several BentoDesk subsystems need
//! re-priming after `WM_POWERBROADCAST` / `PBT_APMRESUMEAUTOMATIC`:
//!
//! 1. The `notify` file watcher's `ReadDirectoryChangesW` handle may be
//!    stale; the watcher needs a teardown + recreate.
//! 2. The ghost-layer overlay HWND may have lost its `HWND_TOPMOST`
//!    z-order to the desktop shell re-initialising.
//! 3. The displays may have changed (external monitor disconnected); the
//!    overlay must reposition to the current work area.
//!
//! 1.x reached for `tauri::AppHandle` to look these subsystems up via
//! `try_state::<...>()`. The native single-process invariant means each
//! recovery side-effect lives in its own crate; this module simply emits
//! a [`PowerEvent::Resumed`] on the supplied [`Sender<PowerEvent>`] and
//! lets the dispatcher (`bentodesk-app::dispatcher`) decide which
//! subsystems to wake.
//!
//! The `WM_POWERBROADCAST` decode itself lives in the wndproc subclass
//! (`bentodesk-shell::window::wndproc`); this module exposes the public
//! [`handle_resume`] entry point that wndproc invokes.

use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

/// Set when a resume recovery thread is running, cleared when it returns.
/// Guards against re-entry when Windows sends multiple resume messages
/// in rapid succession (the OS dispatches one per device class).
static RESUME_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Default delay between the wndproc resume signal and the recovery side
/// effects. Mirrors the 1.x default; lets display drivers, USB devices, and
/// network stacks finish their own post-resume bring-up before we touch
/// dependent subsystems.
pub const DEFAULT_RESUME_DELAY_MS: u32 = 2_000;

// ─── Public surface ──────────────────────────────────────────────────

/// Power event delivered to the dispatcher. Only one variant today; future
/// `WM_POWERBROADCAST` signals (e.g. battery threshold) extend this enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PowerEvent {
    /// The system woke from sleep/hibernate. Subsystems should refresh
    /// state that relies on the OS device-init sequence (file watcher
    /// directory handles, ghost-layer z-order, display geometry).
    Resumed,
}

/// Configuration accepted by [`handle_resume`]. Mirrors the 1.x
/// `AppSettings.{hibernate_resume_delay_ms, safe_start_after_hibernation}`
/// fields without pulling the full settings struct in here.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResumeConfig {
    /// Milliseconds to wait between the resume signal and emitting
    /// [`PowerEvent::Resumed`]. Defaults to [`DEFAULT_RESUME_DELAY_MS`].
    pub delay_ms: u32,
    /// When `false`, [`handle_resume`] silently no-ops. Lets users opt out
    /// of the recovery side effects on systems where they cause more harm
    /// than good.
    pub safe_start_enabled: bool,
}

impl Default for ResumeConfig {
    fn default() -> Self {
        Self {
            delay_ms: DEFAULT_RESUME_DELAY_MS,
            safe_start_enabled: true,
        }
    }
}

/// Handle a power resume event delivered by the wndproc subclass.
///
/// Spawns a fixed-stack `std::thread` worker (single resume in flight at a
/// time, guarded by [`RESUME_IN_PROGRESS`]) which sleeps for
/// `config.delay_ms` then emits [`PowerEvent::Resumed`] on `event_tx`.
///
/// Returns immediately so the wndproc message pump is never blocked.
/// Drops the resume entirely when:
/// - `safe_start_enabled` is `false`, or
/// - a previous resume is still in flight.
pub fn handle_resume(config: ResumeConfig, event_tx: Sender<PowerEvent>) {
    if !config.safe_start_enabled {
        tracing::info!("power: safe_start disabled, skipping resume recovery");
        return;
    }
    if RESUME_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        tracing::info!("power: resume recovery already in flight, dropping duplicate");
        return;
    }

    tracing::info!(
        "power: resume detected, scheduling recovery in {}ms",
        config.delay_ms
    );

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(u64::from(config.delay_ms)));
        if event_tx.send(PowerEvent::Resumed).is_err() {
            tracing::warn!("power: resume event channel closed, recovery skipped");
        }
        RESUME_IN_PROGRESS.store(false, Ordering::SeqCst);
        tracing::info!("power: resume recovery dispatched");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// Tests that touch the global `RESUME_IN_PROGRESS` flag must run
    /// serially; cargo test default is parallel and would race on the static.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn reset_in_flight() {
        RESUME_IN_PROGRESS.store(false, Ordering::SeqCst);
    }

    #[test]
    fn resume_config_default_matches_constants() {
        let cfg = ResumeConfig::default();
        assert_eq!(cfg.delay_ms, DEFAULT_RESUME_DELAY_MS);
        assert!(cfg.safe_start_enabled);
    }

    #[test]
    fn safe_start_disabled_skips_event() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset_in_flight();
        let (tx, rx) = unbounded::<PowerEvent>();
        handle_resume(
            ResumeConfig {
                delay_ms: 0,
                safe_start_enabled: false,
            },
            tx,
        );
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            rx.try_recv().is_err(),
            "expected no event when safe_start is disabled"
        );
    }

    #[test]
    fn enabled_resume_emits_after_delay() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset_in_flight();
        let (tx, rx) = unbounded::<PowerEvent>();
        handle_resume(
            ResumeConfig {
                delay_ms: 25,
                safe_start_enabled: true,
            },
            tx,
        );
        let start = Instant::now();
        let event = rx
            .recv_timeout(Duration::from_millis(500))
            .expect("expected Resumed event");
        let elapsed = start.elapsed();
        assert_eq!(event, PowerEvent::Resumed);
        assert!(
            elapsed >= Duration::from_millis(20),
            "fired too quickly: {elapsed:?}"
        );
        // Wait for the spawned worker to clear the in-flight flag before
        // releasing the serial guard.
        std::thread::sleep(Duration::from_millis(20));
    }

    #[test]
    fn duplicate_resume_in_flight_is_dropped() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset_in_flight();
        let (tx1, rx1) = unbounded::<PowerEvent>();
        let (tx2, rx2) = unbounded::<PowerEvent>();
        handle_resume(
            ResumeConfig {
                delay_ms: 100,
                safe_start_enabled: true,
            },
            tx1,
        );
        // While the first thread is sleeping, attempt a second resume —
        // it must drop without spawning another thread.
        handle_resume(
            ResumeConfig {
                delay_ms: 100,
                safe_start_enabled: true,
            },
            tx2.clone(),
        );
        // Drop tx2 so rx2 will close once the (non-existent) sender goes
        // away. If a second thread had spawned, rx2 would receive an event.
        drop(tx2);
        let _ = rx1.recv_timeout(Duration::from_millis(500));
        assert!(
            rx2.try_recv().is_err(),
            "second resume must not have spawned a worker"
        );
        // Wait for the spawned worker to clear the in-flight flag before
        // releasing the serial guard.
        std::thread::sleep(Duration::from_millis(20));
    }
}
