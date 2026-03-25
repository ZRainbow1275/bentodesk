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

/// Validate that a path resides within the Desktop directory or the `.bentodesk/`
/// hidden directory. Returns `Err` with a descriptive message if the path is
/// outside the allowed boundaries.
fn validate_allowed_path(path: &str, state: &AppState) -> Result<(), String> {
    let canonical = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => {
            // File might not exist yet (e.g. during reveal). Use the raw path
            // normalized as best we can for the check.
            std::path::PathBuf::from(path)
        }
    };

    let canonical_lower = canonical.to_string_lossy().to_lowercase().replace('/', "\\");

    // Allow 1: Desktop directory
    let desktop_path = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.desktop_path.clone()
    };
    let desktop_lower = desktop_path.to_lowercase().replace('/', "\\");
    if canonical_lower.starts_with(&desktop_lower) {
        return Ok(());
    }

    // Allow 2: System desktop dir (fallback)
    if let Some(sys_desktop) = dirs::desktop_dir() {
        let sys_lower = sys_desktop.to_string_lossy().to_lowercase().replace('/', "\\");
        if canonical_lower.starts_with(&sys_lower) {
            return Ok(());
        }
    }

    // Allow 3: BentoDesk app data directory (icons, layout backups)
    if let Some(data_dir) = dirs::data_dir() {
        let app_data = data_dir.join("BentoDesk");
        let app_data_lower = app_data.to_string_lossy().to_lowercase().replace('/', "\\");
        if canonical_lower.starts_with(&app_data_lower) {
            return Ok(());
        }
    }

    Err(format!(
        "Path is outside allowed directories: {}",
        path
    ))
}

#[tauri::command]
pub async fn open_file(state: State<'_, AppState>, path: String) -> Result<(), String> {
    // Security: validate that the file is within an allowed directory
    validate_allowed_path(&path, &state)?;

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
