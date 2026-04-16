//! File operations (open, reveal in Explorer).
//!
//! Security: All path parameters from the frontend are validated to ensure they
//! reside within a BentoDesk-managed location (the user's Desktop directory or
//! the `.bentodesk/` hidden directory). This prevents a compromised WebView from
//! opening or revealing arbitrary system files.

use std::path::Path;

use tauri::State;

use crate::grouping::scanner::FileInfo;
use crate::AppState;

/// File extensions that are directly executable via ShellExecuteW and must be
/// blocked to prevent script injection from the WebView.
const BLOCKED_SCRIPT_EXTENSIONS: &[&str] = &[
    "bat", "cmd", "ps1", "ps2", "vbs", "vbe", "js", "jse", "wsf", "wsh", "msi",
    "scr", "hta", "cpl", "inf", "reg", "pif", "com",
];

/// Strip the Windows extended-length path prefix (`\\?\`) so that
/// path-prefix comparisons work uniformly.
fn strip_unc(s: &str) -> &str {
    s.strip_prefix(r"\\?\").unwrap_or(s)
}

/// Core path-validation logic, decoupled from `AppState` for testability.
/// Resolves the BentoDesk AppData directory via `dirs::data_dir()`; tests
/// that need a deterministic AppData root should call
/// [`validate_allowed_path_with_app_data`] directly.
///
/// `desktop_path` is the user-configured desktop directory. An empty string
/// is treated as "no override" — earlier versions used `starts_with("")`
/// which silently matched every path and effectively disabled validation
/// when the setting was blank, so the explicit empty-string branch below is
/// load-bearing for security.
fn validate_allowed_path_inner(path: &str, desktop_path: &str) -> Result<(), String> {
    let app_data = dirs::data_dir().map(|d| d.join("BentoDesk"));
    validate_allowed_path_with_app_data(path, desktop_path, app_data.as_deref())
}

/// Same as [`validate_allowed_path_inner`] but with an injectable AppData
/// root. Used directly by unit tests so the AppData allow-list can be
/// exercised against a temporary directory rather than the user's real
/// `%APPDATA%\BentoDesk`.
fn validate_allowed_path_with_app_data(
    path: &str,
    desktop_path: &str,
    app_data: Option<&Path>,
) -> Result<(), String> {
    let canonical = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => {
            // File might not exist yet (e.g. during reveal). Fall back to the
            // raw path, but reject any path containing ".." components to
            // prevent directory traversal bypasses.
            let raw = std::path::PathBuf::from(path);
            if raw.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                return Err(format!(
                    "Path contains disallowed parent directory traversal (\"..\"): {path}"
                ));
            }
            raw
        }
    };

    // B5 fix: normalize BOTH separators AND the extended-length prefix before
    // any prefix comparison, otherwise e.g. "C:/Users/.." forms slip past
    // checks that were written with backslashes in mind.
    let canonical_lower = strip_unc(&canonical.to_string_lossy())
        .to_lowercase()
        .replace('/', "\\");

    let norm = |s: &str| {
        s.to_lowercase()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_string()
    };

    // Match either exact equality or "<source>\..." so that a desktop
    // root of "c:\users\alice\desktop" cannot be bypassed by a sibling
    // directory like "c:\users\alice\desktopbackup\evil.exe".
    let is_inside = |root: &str, candidate: &str| -> bool {
        if root.is_empty() {
            return false;
        }
        candidate == root || candidate.starts_with(&format!("{root}\\"))
    };

    // Allow list 1: every legitimate Desktop source (user, public, OneDrive,
    // plus the settings override).
    let custom = if desktop_path.trim().is_empty() {
        None
    } else {
        Some(desktop_path)
    };
    for source in crate::desktop_sources::all_desktop_dirs(custom) {
        let source_lower = norm(&source.to_string_lossy());
        if is_inside(&source_lower, &canonical_lower) {
            return Ok(());
        }
    }

    // Allow list 2: BentoDesk app data directory (icons, layout backups).
    if let Some(app_data) = app_data {
        let app_data_lower = norm(&app_data.to_string_lossy());
        if is_inside(&app_data_lower, &canonical_lower) {
            return Ok(());
        }
    }

    Err(format!(
        "Path is outside allowed directories: {}",
        path
    ))
}

/// Validate that a path resides within the Desktop directory or the `.bentodesk/`
/// hidden directory. Returns `Err` with a descriptive message if the path is
/// outside the allowed boundaries.
fn validate_allowed_path(path: &str, state: &AppState) -> Result<(), String> {
    let desktop_path = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.desktop_path.clone()
    };
    validate_allowed_path_inner(path, &desktop_path)
}

#[tauri::command]
pub async fn open_file(state: State<'_, AppState>, path: String) -> Result<(), String> {
    // Security: validate that the file is within an allowed directory
    validate_allowed_path(&path, &state)?;

    // Security: reject executable script types to prevent script injection
    if let Some(ext) = Path::new(&path).extension() {
        let ext_lower = ext.to_string_lossy().to_lowercase();
        if BLOCKED_SCRIPT_EXTENSIONS.contains(&ext_lower.as_str()) {
            return Err(format!(
                "Cannot open executable script files (.{ext_lower}) for security reasons"
            ));
        }
    }

    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let wide_path: Vec<u16> = OsStr::new(&path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let wide_open: Vec<u16> = OsStr::new("open")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: ShellExecuteW is a well-documented Shell API. We pass valid
    // null-terminated wide strings.
    let result = unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            None,
            windows::core::PCWSTR(wide_open.as_ptr()),
            windows::core::PCWSTR(wide_path.as_ptr()),
            None,
            None,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };

    // ShellExecuteW returns a value > 32 on success
    if result.0 as usize <= 32 {
        return Err(format!("Failed to open file: {path}"));
    }

    Ok(())
}

#[tauri::command]
pub async fn reveal_in_explorer(state: State<'_, AppState>, path: String) -> Result<(), String> {
    // Security: validate that the file is within an allowed directory
    validate_allowed_path(&path, &state)?;

    // Use `explorer.exe /select,<path>` to open Explorer with the file selected.
    // The path is passed as a single pre-formatted argument which is safe because
    // explorer.exe /select, expects exactly this format and does not invoke a shell.
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{path}"))
        .spawn()
        .map_err(|e| format!("Failed to reveal in Explorer: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn get_file_info(state: State<'_, AppState>, path: String) -> Result<FileInfo, String> {
    // Security: validate that the path is within allowed directories
    validate_allowed_path(&path, &state)?;

    let file_path = Path::new(&path);
    let metadata = std::fs::metadata(file_path).map_err(|e| e.to_string())?;

    let name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let extension = file_path
        .extension()
        .map(|e| e.to_string_lossy().to_string().to_lowercase());

    let file_type = if metadata.is_dir() {
        "folder".to_string()
    } else {
        extension.clone().unwrap_or_else(|| "unknown".to_string())
    };

    let modified_at = metadata
        .modified()
        .ok()
        .map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            dt.to_rfc3339()
        })
        .unwrap_or_default();

    let created_at = metadata
        .created()
        .ok()
        .map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            dt.to_rfc3339()
        })
        .unwrap_or_default();

    Ok(FileInfo {
        name,
        path,
        size: metadata.len(),
        file_type,
        modified_at,
        created_at,
        is_directory: metadata.is_dir(),
        extension,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_allowed_path_inner (P2-8: path traversal prevention) ──

    #[test]
    fn rejects_path_with_parent_dir_traversal() {
        // Non-existent path with ".." must be rejected
        let result = validate_allowed_path_inner(
            r"C:\Users\Desktop\..\Windows\System32\cmd.exe",
            r"C:\Users\Desktop",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(".."));
    }

    #[test]
    fn rejects_forward_slash_traversal() {
        let result = validate_allowed_path_inner(
            "C:/Users/Desktop/../Windows/System32/cmd.exe",
            r"C:\Users\Desktop",
        );
        assert!(result.is_err());
    }

    #[test]
    fn allows_file_inside_desktop_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let desktop = tmp.path();
        let file = desktop.join("test.txt");
        std::fs::write(&file, "hello").unwrap();

        let result = validate_allowed_path_inner(
            &file.to_string_lossy(),
            &desktop.to_string_lossy(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn allows_file_inside_bentodesk_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let desktop = tmp.path();
        let bentodesk = desktop.join(".bentodesk").join("zone-1");
        std::fs::create_dir_all(&bentodesk).unwrap();
        let file = bentodesk.join("hidden.txt");
        std::fs::write(&file, "hidden").unwrap();

        let result = validate_allowed_path_inner(
            &file.to_string_lossy(),
            &desktop.to_string_lossy(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_path_outside_desktop() {
        let tmp_desktop = tempfile::tempdir().unwrap();
        let tmp_other = tempfile::tempdir().unwrap();
        let outside_file = tmp_other.path().join("evil.txt");
        std::fs::write(&outside_file, "evil").unwrap();

        let result = validate_allowed_path_inner(
            &outside_file.to_string_lossy(),
            &tmp_desktop.path().to_string_lossy(),
        );
        // This should fail unless the file happens to be under system desktop or AppData.
        // Since tempdir is in %TEMP%, it should be rejected.
        assert!(result.is_err());
    }

    #[test]
    fn rejects_nonexistent_path_outside_desktop_without_traversal() {
        // Non-existent path without ".." but outside desktop — should be rejected
        // because the raw path won't start with the desktop prefix.
        let result = validate_allowed_path_inner(
            r"C:\Windows\System32\nonexistent.txt",
            r"C:\Users\TestUser\Desktop",
        );
        assert!(result.is_err());
    }

    #[test]
    fn allows_file_inside_injected_appdata_dir() {
        // Desktop set to an unrelated tempdir so the file cannot match
        // Allow list 1; only Allow list 2 (AppData) can authorize it.
        let desktop = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let nested = app_data.path().join("icons").join("cache");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("ok.png");
        std::fs::write(&file, [0u8]).unwrap();

        let result = validate_allowed_path_with_app_data(
            &file.to_string_lossy(),
            &desktop.path().to_string_lossy(),
            Some(app_data.path()),
        );
        assert!(result.is_ok(), "AppData-rooted file must be allowed: {result:?}");
    }

    #[test]
    fn rejects_appdata_sibling_via_prefix_collision() {
        // Regression guard for the prefix-collision bug: if AppData is
        // "c:\…\BentoDesk", a sibling "BentoDeskEvil" must NOT be allowed.
        let desktop = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let allowed = parent.path().join("BentoDesk");
        let evil = parent.path().join("BentoDeskEvil");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&evil).unwrap();
        let evil_file = evil.join("payload.bin");
        std::fs::write(&evil_file, [0u8]).unwrap();

        let result = validate_allowed_path_with_app_data(
            &evil_file.to_string_lossy(),
            &desktop.path().to_string_lossy(),
            Some(&allowed),
        );
        assert!(result.is_err(), "Sibling directory must not bypass AppData allow-list");
    }

    #[test]
    fn rejects_desktop_sibling_via_prefix_collision() {
        // Same regression guard for Allow list 1: "Desktop" must not match
        // "DesktopBackup".
        let desktop_root = tempfile::tempdir().unwrap();
        let desktop = desktop_root.path().join("Desktop");
        let evil = desktop_root.path().join("DesktopBackup");
        std::fs::create_dir_all(&desktop).unwrap();
        std::fs::create_dir_all(&evil).unwrap();
        let evil_file = evil.join("payload.bin");
        std::fs::write(&evil_file, [0u8]).unwrap();

        let result = validate_allowed_path_with_app_data(
            &evil_file.to_string_lossy(),
            &desktop.to_string_lossy(),
            None,
        );
        assert!(result.is_err(), "Sibling directory must not bypass Desktop allow-list");
    }

    // ── BLOCKED_SCRIPT_EXTENSIONS (P2-6: script injection prevention) ──

    #[test]
    fn blocks_all_dangerous_script_extensions() {
        let dangerous = vec![
            "bat", "cmd", "ps1", "ps2", "vbs", "vbe", "js", "jse",
            "wsf", "wsh", "msi", "scr", "hta", "cpl", "inf", "reg",
            "pif", "com",
        ];
        for ext in &dangerous {
            assert!(
                BLOCKED_SCRIPT_EXTENSIONS.contains(ext),
                "Extension '.{ext}' should be blocked but is not in BLOCKED_SCRIPT_EXTENSIONS"
            );
        }
    }

    #[test]
    fn does_not_block_safe_extensions() {
        let safe = vec!["txt", "pdf", "png", "jpg", "docx", "xlsx", "lnk", "exe"];
        for ext in &safe {
            assert!(
                !BLOCKED_SCRIPT_EXTENSIONS.contains(ext),
                "Extension '.{ext}' should NOT be in BLOCKED_SCRIPT_EXTENSIONS"
            );
        }
    }

    #[test]
    fn script_extension_check_is_case_insensitive_in_open_file_logic() {
        // Simulate the case-insensitive check used in open_file
        let test_cases = vec![
            ("test.BAT", true),
            ("test.Cmd", true),
            ("test.PS1", true),
            ("test.txt", false),
            ("test.PDF", false),
        ];
        for (filename, should_block) in test_cases {
            let ext = Path::new(filename).extension().unwrap();
            let ext_lower = ext.to_string_lossy().to_lowercase();
            let blocked = BLOCKED_SCRIPT_EXTENSIONS.contains(&ext_lower.as_str());
            assert_eq!(
                blocked, should_block,
                "File '{filename}': expected blocked={should_block}, got blocked={blocked}"
            );
        }
    }

    #[test]
    fn file_without_extension_is_not_blocked() {
        let path = Path::new("README");
        assert!(path.extension().is_none());
    }
}
