//! Startup management via Windows Task Scheduler.
//!
//! Replaces the legacy `HKCU\...\Run` registry approach with `schtasks.exe`
//! for more control over startup timing and Guardian integration.
//! Also provides [`cleanup_legacy_registry`] to remove old registry entries
//! during migration.

use std::fs::File;
use std::io::Read;
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
const TASK_SCHEDULER_TR_MAX_LEN: usize = 261;

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

fn guardian_binary_is_usable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    let mut signature = [0_u8; 2];
    match File::open(path).and_then(|mut file| file.read_exact(&mut signature)) {
        Ok(()) => signature == *b"MZ",
        Err(_) => false,
    }
}

fn task_scheduler_path(path: &Path) -> String {
    fn replace_with_env_var(path: &Path, env_var: &str) -> Option<String> {
        let prefix = std::env::var(env_var).ok()?.replace('/', "\\");
        let raw = path.display().to_string().replace('/', "\\");
        if raw.len() < prefix.len() || !raw[..prefix.len()].eq_ignore_ascii_case(&prefix) {
            return None;
        }

        let suffix = raw[prefix.len()..].trim_start_matches('\\');
        Some(if suffix.is_empty() {
            format!("%{env_var}%")
        } else {
            format!("%{env_var}%\\{suffix}")
        })
    }

    replace_with_env_var(path, "LOCALAPPDATA")
        .or_else(|| replace_with_env_var(path, "APPDATA"))
        .unwrap_or_else(|| path.display().to_string())
}

fn startup_command_value(
    use_guardian: bool,
    app_exe: &Path,
    guardian_exe: &Path,
    app_data: &Path,
    crash_settings: &CrashSettings,
) -> String {
    let app_command = format!("\"{}\"", task_scheduler_path(app_exe));

    if use_guardian {
        if guardian_binary_is_usable(guardian_exe) {
            let safe_mode_flag = app_data.join("safe_mode.json");
            let guardian_command = format!(
                "\"{}\" --main-exe \"{}\" --max-crashes {} --window {} --safe-mode-flag \"{}\"",
                task_scheduler_path(guardian_exe),
                task_scheduler_path(app_exe),
                crash_settings.max_retries,
                crash_settings.window_secs,
                task_scheduler_path(&safe_mode_flag),
            );

            if guardian_command.len() <= TASK_SCHEDULER_TR_MAX_LEN {
                return guardian_command;
            }

            tracing::warn!(
                "Guardian startup command is {} chars (> {}); falling back to main executable",
                guardian_command.len(),
                TASK_SCHEDULER_TR_MAX_LEN
            );
        } else {
            tracing::warn!(
                "Guardian requested for startup, but binary is missing or invalid at {}; falling back to main executable",
                guardian_exe.display()
            );
        }
    }

    app_command
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
    let tr_value = startup_command_value(
        use_guardian,
        app_exe,
        guardian_exe,
        app_data,
        crash_settings,
    );

    let mut args = vec![
        "/create".to_string(),
        "/tn".to_string(),
        TASK_NAME.to_string(),
        "/tr".to_string(),
        tr_value.clone(),
        "/sc".to_string(),
        "onlogon".to_string(),
        "/rl".to_string(),
        "limited".to_string(),
        "/f".to_string(),
    ];

    // Normal priority: add 30-second delay to reduce boot contention.
    if !high_priority {
        args.push("/delay".to_string());
        args.push("0000:30".to_string());
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
        match set_registry_run_fallback(&tr_value) {
            Ok(()) => {
                tracing::warn!("Task Scheduler creation failed; applied HKCU Run fallback instead");
                Ok(())
            }
            Err(registry_err) => Err(BentoDeskError::StartupError(format!(
                "schtasks /create failed: {}; registry fallback failed: {}",
                stderr.trim(),
                registry_err
            ))),
        }
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

fn set_registry_run_fallback(command_line: &str) -> Result<(), BentoDeskError> {
    tracing::info!(
        "Applying HKCU Run startup fallback for '{}' with command: {}",
        TASK_NAME,
        command_line
    );

    let output = Command::new("reg.exe")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            TASK_NAME,
            "/t",
            "REG_EXPAND_SZ",
            "/d",
            command_line,
            "/f",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| BentoDeskError::StartupError(format!("Failed to execute reg.exe add: {e}")))?;

    if output.status.success() {
        tracing::info!("HKCU Run fallback applied for '{}'", TASK_NAME);
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(BentoDeskError::StartupError(format!(
            "reg.exe add failed: {}",
            stderr.trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::tempdir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

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

    #[test]
    fn task_scheduler_path_uses_localappdata_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempdir().unwrap();
        let local_app_data = tmp.path().join("local");
        let app_data = tmp.path().join("roaming");
        let app_exe = local_app_data
            .join("Programs")
            .join("BentoDesk")
            .join("BentoDesk.exe");
        let original_local = std::env::var_os("LOCALAPPDATA");
        let original_appdata = std::env::var_os("APPDATA");

        std::fs::create_dir_all(app_exe.parent().unwrap()).unwrap();
        std::env::set_var("LOCALAPPDATA", &local_app_data);
        std::env::set_var("APPDATA", &app_data);

        let shortened = task_scheduler_path(&app_exe);

        match original_local {
            Some(value) => std::env::set_var("LOCALAPPDATA", value),
            None => std::env::remove_var("LOCALAPPDATA"),
        }
        match original_appdata {
            Some(value) => std::env::set_var("APPDATA", value),
            None => std::env::remove_var("APPDATA"),
        }

        assert_eq!(
            shortened,
            r"%LOCALAPPDATA%\Programs\BentoDesk\BentoDesk.exe"
        );
    }

    #[test]
    fn task_scheduler_path_falls_back_to_appdata_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempdir().unwrap();
        let roaming_app_data = tmp.path().join("roaming");
        let safe_mode_flag = roaming_app_data.join("BentoDesk").join("safe_mode.json");
        let original_local = std::env::var_os("LOCALAPPDATA");
        let original_appdata = std::env::var_os("APPDATA");

        std::fs::create_dir_all(safe_mode_flag.parent().unwrap()).unwrap();
        std::env::remove_var("LOCALAPPDATA");
        std::env::set_var("APPDATA", &roaming_app_data);

        let shortened = task_scheduler_path(&safe_mode_flag);

        match original_local {
            Some(value) => std::env::set_var("LOCALAPPDATA", value),
            None => std::env::remove_var("LOCALAPPDATA"),
        }
        match original_appdata {
            Some(value) => std::env::set_var("APPDATA", value),
            None => std::env::remove_var("APPDATA"),
        }

        assert_eq!(shortened, r"%APPDATA%\BentoDesk\safe_mode.json");
    }

    #[test]
    fn guardian_binary_is_usable_accepts_pe_signature() {
        let tmp = tempdir().unwrap();
        let guardian = tmp.path().join("guardian.exe");
        std::fs::write(&guardian, b"MZ\x90\x00").unwrap();

        assert!(guardian_binary_is_usable(&guardian));
    }

    #[test]
    fn guardian_binary_is_usable_rejects_placeholder_file() {
        let tmp = tempdir().unwrap();
        let guardian = tmp.path().join("guardian.exe");
        std::fs::write(&guardian, b"placeholder").unwrap();

        assert!(!guardian_binary_is_usable(&guardian));
    }

    #[test]
    fn startup_command_value_falls_back_to_app_when_guardian_invalid() {
        let tmp = tempdir().unwrap();
        let app_exe = tmp.path().join("bentodesk.exe");
        let guardian_exe = tmp.path().join("guardian.exe");
        let app_data = tmp.path().join("appdata");
        std::fs::write(&guardian_exe, b"placeholder").unwrap();

        let tr_value = startup_command_value(
            true,
            &app_exe,
            &guardian_exe,
            &app_data,
            &CrashSettings {
                max_retries: 3,
                window_secs: 10,
            },
        );

        assert_eq!(tr_value, format!("\"{}\"", task_scheduler_path(&app_exe)));
    }

    #[test]
    fn startup_command_value_uses_guardian_when_valid() {
        let tmp = tempdir().unwrap();
        let app_exe = tmp.path().join("bentodesk.exe");
        let guardian_exe = tmp.path().join("guardian.exe");
        let app_data = tmp.path().join("appdata");
        std::fs::write(&guardian_exe, b"MZ\x90\x00").unwrap();

        let tr_value = startup_command_value(
            true,
            &app_exe,
            &guardian_exe,
            &app_data,
            &CrashSettings {
                max_retries: 5,
                window_secs: 12,
            },
        );

        assert!(tr_value.contains(&task_scheduler_path(&guardian_exe)));
        assert!(tr_value.contains("--main-exe"));
        assert!(tr_value.contains("--max-crashes 5"));
        assert!(tr_value.contains("--window 12"));
        assert!(tr_value.contains("safe_mode.json"));
    }

    #[test]
    fn startup_command_value_falls_back_when_guardian_command_exceeds_scheduler_limit() {
        let tmp = tempdir().unwrap();
        let app_exe = tmp.path().join("bentodesk.exe");
        let guardian_exe = tmp.path().join("guardian.exe");
        let very_long_app_data = Path::new(
            r"C:\very\long\path\segment\segment\segment\segment\segment\segment\segment\segment\segment\segment",
        );
        std::fs::write(&guardian_exe, b"MZ\x90\x00").unwrap();

        let tr_value = startup_command_value(
            true,
            &app_exe,
            &guardian_exe,
            very_long_app_data,
            &CrashSettings {
                max_retries: 3,
                window_secs: 10,
            },
        );

        assert_eq!(tr_value, format!("\"{}\"", task_scheduler_path(&app_exe)));
    }
}
