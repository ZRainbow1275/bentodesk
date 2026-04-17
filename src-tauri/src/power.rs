//! Power event handling for BentoDesk — hibernate/sleep resume recovery.
//!
//! When a laptop sleeps or hibernates, the `notify` file watcher may lose
//! its directory handle and miss events. The ghost layer window may also be
//! pushed out of `HWND_BOTTOM` position by the desktop shell re-initializing.
//! Display configuration can change (e.g. external monitor disconnected).
//!
//! This module processes `WM_POWERBROADCAST` / `PBT_APMRESUMEAUTOMATIC`
//! events to restore BentoDesk to a consistent state after resume.
//!
//! ## Recovery actions on resume
//!
//! 1. Re-assert `SetWindowPos(HWND_BOTTOM)` for the ghost layer
//! 2. `reposition_to_work_area()` to adapt to display changes
//! 3. Rebuild the file watcher (or trigger a full reconciliation scan)

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether a power resume event is currently being handled.
/// Prevents duplicate handling if multiple resume messages arrive.
static RESUME_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Handle a power resume event. Called from the WndProc subclass when
/// `WM_POWERBROADCAST` with `PBT_APMRESUMEAUTOMATIC` is received.
///
/// This runs on the window message thread, so heavy work is dispatched
/// to a background thread to avoid blocking the message pump.
pub fn handle_resume(app_handle: tauri::AppHandle) {
    // Guard against duplicate resume handling
    if RESUME_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }

    tracing::info!("Power resume detected — scheduling recovery");

    std::thread::spawn(move || {
        // Read configurable delay and safe-start toggle from settings.
        let (delay_ms, safe_start) = {
            use tauri::Manager;
            app_handle
                .try_state::<crate::AppState>()
                .and_then(|state| {
                    state
                        .settings
                        .lock()
                        .ok()
                        .map(|s| (s.hibernate_resume_delay_ms, s.safe_start_after_hibernation))
                })
                .unwrap_or((2000, true))
        };

        if !safe_start {
            tracing::info!(
                "Power resume: safe_start_after_hibernation is disabled, skipping recovery"
            );
            RESUME_IN_PROGRESS.store(false, Ordering::SeqCst);
            return;
        }

        // Configurable delay to let the system stabilize after resume.
        // Display drivers, USB devices, and network may still be initializing.
        tracing::info!(
            "Power resume: waiting {}ms for system stabilization",
            delay_ms
        );
        std::thread::sleep(std::time::Duration::from_millis(u64::from(delay_ms)));

        // 1. Re-assert ghost layer z-order and position
        tracing::info!("Power resume: re-asserting ghost layer position");
        crate::ghost_layer::manager::show_window();
        crate::ghost_layer::manager::reposition_to_work_area();

        // 2. Rebuild the file watcher
        // The notify crate's ReadDirectoryChangesW handle may be stale
        // after hibernate. Drop the old watcher and create a new one.
        tracing::info!("Power resume: rebuilding file watcher");
        rebuild_file_watcher(&app_handle);

        // 3. Emit a frontend event so the UI can refresh if needed
        if let Err(e) = tauri::Emitter::emit(&app_handle, "power_resume", ()) {
            tracing::warn!("Failed to emit power_resume event: {e}");
        }

        RESUME_IN_PROGRESS.store(false, Ordering::SeqCst);
        tracing::info!("Power resume recovery complete");
    });
}

/// Drop the existing file watcher and create a new one.
///
/// The `notify` crate on Windows uses `ReadDirectoryChangesW` which can
/// become invalid after hibernate/sleep. Rebuilding ensures we don't
/// silently miss file changes.
fn rebuild_file_watcher(app_handle: &tauri::AppHandle) {
    use tauri::Manager;

    // Access the existing WatcherState and drop the old debouncer
    if let Some(state) = app_handle.try_state::<crate::watcher::desktop_watcher::WatcherState>() {
        if let Ok(mut debouncer) = state.debouncer.lock() {
            // Drop the old watcher by taking it out of the Option
            let _ = debouncer.take();
            tracing::info!("Old file watcher dropped");
        }
    }

    // Set up a fresh watcher
    match crate::watcher::desktop_watcher::setup_file_watcher_inner(app_handle) {
        Ok(new_debouncer) => {
            if let Some(state) =
                app_handle.try_state::<crate::watcher::desktop_watcher::WatcherState>()
            {
                if let Ok(mut debouncer) = state.debouncer.lock() {
                    *debouncer = Some(new_debouncer);
                    tracing::info!("File watcher rebuilt successfully");
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to rebuild file watcher after resume: {e}");
        }
    }
}
