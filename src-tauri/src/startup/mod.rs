//! Startup management via Windows Task Scheduler.
//!
//! Replaces the legacy `HKCU\...\Run` registry approach with `schtasks.exe`
//! for more control over startup timing and Guardian integration.
//! Also provides [`cleanup_legacy_registry`] to remove old registry entries
//! during migration.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::error::BentoDeskError;

/// Win32 `CREATE_NO_WINDOW` flag — prevents console window flash for child processes.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Crash detection settings forwarded to the Guardian process.
#[derive(Debug, Clone)]
pub struct CrashSettings {
    /// Maximum crash retries within the crash window.
    pub max_retries: u32,
    /// Crash detection window in seconds.
    pub window_secs: u32,
}

/// Task Scheduler task name used by BentoDesk.
const TASK_NAME: &str = "BentoDesk";

/// Configure the Windows Task Scheduler entry for BentoDesk startup.
///
/// When `enabled` is `true`, creates (or replaces) a Task Scheduler task that
/// launches BentoDesk at user logon. When `false`, deletes the task.
///
/// # Arguments
///
/// * `enabled` — Whether startup should be active.
/// * `high_priority` — If `true`, starts immediately at logon. If `false`,
///   delays 30 seconds to reduce boot contention.
/// * `use_guardian` — If `true`, the scheduled task launches `guardian.exe`
///   which in turn monitors and restarts the main application.
/// * `app_exe` — Path to the main BentoDesk executable.
/// * `guardian_exe` — Path to the Guardian watchdog executable.
/// * `app_data` — Application data directory (used for safe-mode flag path).
/// * `crash_settings` — Guardian crash detection parameters.
pub fn configure(
    enabled: bool,
    high_priority: bool,
    use_guardian: bool,
    app_exe: &Path,
    guardian_exe: &Path,
    app_data: &Path,
    crash_settings: &CrashSettings,
) -> Result<(), BentoDeskError> {
    if enabled {
        create_task(
            high_priority,
            use_guardian,
            app_exe,
            guardian_exe,
            app_data,
            crash_settings,
        )
    } else {
        delete_task()
    }
}

/// Create or replace the Task Scheduler task for BentoDesk startup.
fn create_task(
    high_priority: bool,
    use_guardian: bool,
    app_exe: &Path,
    guardian_exe: &Path,
    app_data: &Path,
    crash_settings: &CrashSettings,
) -> Result<(), BentoDeskError> {
    let tr_value = if use_guardian {
        let safe_mode_flag = app_data.join("safe_mode.json");
        format!(
            "\"{}\" --main-exe \"{}\" --max-crashes {} --window {} --safe-mode-flag \"{}\"",
            guardian_exe.display(),
            app_exe.display(),
            crash_settings.max_retries,
            crash_settings.window_secs,
            safe_mode_flag.display(),
        )
    } else {
        format!("\"{}\"", app_exe.display())
    };

    let mut args = vec![
        "/create".to_string(),
        "/tn".to_string(),
        TASK_NAME.to_string(),
        "/tr".to_string(),
        tr_value,
        "/sc".to_string(),
        "onlogon".to_string(),
        "/rl".to_string(),
        "limited".to_string(),
        "/f".to_string(),
    ];

    // Normal priority: add 30-second delay to reduce boot contention.
    if !high_priority {
        args.push("/delay".to_string());
        args.push("0000:00:30".to_string());
    }

    tracing::info!("Creating Task Scheduler task: schtasks {}", args.join(" "));

    let output = Command::new("schtasks.exe")
        .args(&args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| {
            BentoDeskError::StartupError(format!("Failed to execute schtasks.exe: {e}"))
        })?;

    if output.status.success() {
        tracing::info!(
            "Task Scheduler task '{}' created (high_priority={}, guardian={})",
            TASK_NAME,
            high_priority,
            use_guardian,
        );
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!(
            "schtasks /create failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim(),
        );
        Err(BentoDeskError::StartupError(format!(
            "schtasks /create failed: {}",
            stderr.trim()
        )))
    }
}

/// Delete the BentoDesk Task Scheduler task.
fn delete_task() -> Result<(), BentoDeskError> {
    tracing::info!("Deleting Task Scheduler task '{}'", TASK_NAME);

    let output = Command::new("schtasks.exe")
        .args(["/delete", "/tn", TASK_NAME, "/f"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| {
            BentoDeskError::StartupError(format!("Failed to execute schtasks.exe: {e}"))
        })?;

    if output.status.success() {
        tracing::info!("Task Scheduler task '{}' deleted", TASK_NAME);
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If the task doesn't exist, treat as success (idempotent delete).
        if stderr.to_lowercase().contains("cannot find")
            || stderr.to_lowercase().contains("does not exist")
        {
            tracing::info!("Task '{}' did not exist, nothing to delete", TASK_NAME);
            Ok(())
        } else {
            tracing::error!(
                "schtasks /delete failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr.trim(),
            );
            Err(BentoDeskError::StartupError(format!(
                "schtasks /delete failed: {}",
                stderr.trim()
            )))
        }
    }
}

/// Remove the legacy `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\BentoDesk`
/// registry value, if it exists.
///
/// Called once during migration from the old registry-based startup to the new
/// Task Scheduler approach. Errors are logged as warnings but do not fail the
/// operation, since the legacy key may already be absent.
pub fn cleanup_legacy_registry() -> Result<(), BentoDeskError> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, HKEY_CURRENT_USER, KEY_WRITE,
    };

    let sub_key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
        .encode_utf16()
        .collect();
    let value_name: Vec<u16> = "BentoDesk\0".encode_utf16().collect();

    // SAFETY: RegOpenKeyExW and RegDeleteValueW are well-documented Registry
    // APIs. We pass valid null-terminated wide-string pointers and a properly
    // initialised HKEY output parameter. The key is closed via RegCloseKey
    // after the operation.
    unsafe {
        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub_key.as_ptr()),
            0,
            KEY_WRITE,
            &mut hkey,
        );

        if status.is_err() {
            // Key does not exist or cannot be opened — nothing to clean up.
            tracing::info!("Legacy Run key not accessible, skipping cleanup");
            return Ok(());
        }

        let del_result = RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()));
        let _ = RegCloseKey(hkey);

        if del_result.is_ok() {
            tracing::info!("Cleaned up legacy registry Run key for BentoDesk");
        } else {
            // Value may not exist — this is fine.
            tracing::info!("Legacy BentoDesk Run value not found, nothing to remove");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_settings_defaults() {
        let cs = CrashSettings {
            max_retries: 3,
            window_secs: 10,
        };
        assert_eq!(cs.max_retries, 3);
        assert_eq!(cs.window_secs, 10);
    }

    #[test]
    fn task_name_is_bentodesk() {
        assert_eq!(TASK_NAME, "BentoDesk");
    }
}
