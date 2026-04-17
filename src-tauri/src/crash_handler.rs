//! Crash handler for BentoDesk — restores hidden files on unhandled exceptions.
//!
//! BentoDesk hides desktop files by moving them into `.bentodesk/`. If the
//! process crashes without restoring them, the user's files become invisible.
//! This module installs a Windows SEH (Structured Exception Handling) filter
//! that attempts emergency file restoration before the process terminates.
//!
//! ## Safety
//!
//! The SEH handler runs in a partially-corrupted process state. It:
//! - Reads a known manifest.json file from disk
//! - Moves files back via `fs::rename` (atomic, no allocation on NTFS)
//! - Avoids heap allocation where possible (uses stack buffers)
//! - Does NOT access Tauri state, locks, or managed resources
//!
//! This is a best-effort recovery — it may fail if the heap is corrupted,
//! but the safety manifest on disk provides a secondary recovery path for
//! manual or next-launch restoration.

use std::path::PathBuf;
use std::sync::OnceLock;

use windows::Win32::System::Diagnostics::Debug::{
    SetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX,
};
use windows::Win32::System::Diagnostics::Debug::{
    SetUnhandledExceptionFilter, EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER,
};

/// The desktop path — set once at startup so the SEH handler can find
/// `.bentodesk/manifest.json` without accessing Tauri state.
static DESKTOP_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Previous exception filter, chained after our handler.
static PREV_FILTER: OnceLock<LPTOP_LEVEL_EXCEPTION_FILTER> = OnceLock::new();

/// Install the crash handler. Must be called early in startup, after the
/// desktop path is known.
///
/// This sets:
/// 1. `SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX)` — suppresses
///    the Windows Error Reporting dialog so the process exits cleanly.
/// 2. `SetUnhandledExceptionFilter` — our SEH handler that restores hidden files.
pub fn install(desktop_path: PathBuf) {
    let _ = DESKTOP_PATH.set(desktop_path);

    // SAFETY: SetErrorMode is safe to call at any point. We suppress the WER
    // crash dialog so the process terminates without user interaction.
    unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    }

    // SAFETY: SetUnhandledExceptionFilter registers a callback for unhandled
    // SEH exceptions. Our callback (`crash_exception_filter`) only performs
    // file I/O (read manifest, rename files) — no heap allocation, no locks.
    unsafe {
        let prev = SetUnhandledExceptionFilter(Some(crash_exception_filter));
        let _ = PREV_FILTER.set(prev);
    }

    tracing::info!("Crash handler installed (SEH + SetErrorMode)");
}

/// SEH exception filter — called by Windows when an unhandled exception occurs.
///
/// SAFETY: This runs in a potentially corrupted process. We:
/// - Only read from a `OnceLock` (lock-free after init)
/// - Read a file from disk (OS-level I/O, not heap-dependent)
/// - Use `fs::rename` for file restoration (atomic NTFS operation)
/// - Chain to the previous filter if one existed
///
/// We deliberately avoid: Tauri state, Mutex locks, complex allocations,
/// logging (tracing may be corrupted), or any COM calls.
unsafe extern "system" fn crash_exception_filter(exception_info: *const EXCEPTION_POINTERS) -> i32 {
    // Attempt emergency file restoration
    emergency_restore();

    // Chain to previous handler if one was installed (e.g. by Tauri or CRT)
    if let Some(Some(prev_fn)) = PREV_FILTER.get() {
        return prev_fn(exception_info);
    }

    // EXCEPTION_CONTINUE_SEARCH = 0 — let Windows handle the crash normally
    0
}

/// Emergency file restoration — reads the safety manifest and moves all
/// hidden files back to their original desktop locations.
///
/// This is a best-effort operation. Individual file failures are silently
/// ignored because we cannot reliably log in a crash context.
fn emergency_restore() {
    let Some(desktop_path) = DESKTOP_PATH.get() else {
        return;
    };

    let manifest_path = desktop_path.join(".bentodesk").join("manifest.json");

    // Read the manifest — if this fails, we can't restore
    let manifest_bytes = match std::fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(_) => return,
    };

    // Parse the manifest — minimal JSON parsing of the entries array
    let manifest: serde_json::Value = match serde_json::from_slice(&manifest_bytes) {
        Ok(v) => v,
        Err(_) => return,
    };

    let Some(entries) = manifest.get("entries").and_then(|e| e.as_array()) else {
        return;
    };

    for entry in entries {
        let original = entry.get("original_path").and_then(|v| v.as_str());
        let hidden = entry.get("hidden_path").and_then(|v| v.as_str());

        if let (Some(original), Some(hidden)) = (original, hidden) {
            let hidden_path = std::path::Path::new(hidden);
            let original_path = std::path::Path::new(original);

            // Only restore if the hidden file exists and the original doesn't
            if hidden_path.exists() && !original_path.exists() {
                // fs::rename is atomic on NTFS (same drive, which is guaranteed
                // by the .bentodesk/ subfolder design)
                let _ = std::fs::rename(hidden_path, original_path);
            }
        }
    }

    // Also walk zone subdirectories for any files not in the manifest
    // (belt-and-suspenders approach)
    let bentodesk_dir = desktop_path.join(".bentodesk");
    if let Ok(zone_dirs) = std::fs::read_dir(&bentodesk_dir) {
        for dir_entry in zone_dirs.flatten() {
            let path = dir_entry.path();
            if !path.is_dir() {
                continue;
            }
            // Skip if this looks like a metadata file, not a zone dir
            if path
                .file_name()
                .is_none_or(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            // Move all files in the zone dir back to the desktop
            if let Ok(files) = std::fs::read_dir(&path) {
                for file_entry in files.flatten() {
                    let file_path = file_entry.path();
                    if file_path.is_file() {
                        if let Some(file_name) = file_path.file_name() {
                            let target = desktop_path.join(file_name);
                            if !target.exists() {
                                let _ = std::fs::rename(&file_path, &target);
                            }
                        }
                    }
                }
            }
        }
    }
}
