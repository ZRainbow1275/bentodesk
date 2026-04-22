//! Desktop file scanner.
//!
//! Reads the user's Desktop directory and returns metadata for every file
//! and folder present, which the suggestion engine then analyses.

use serde::Serialize;
use std::path::Path;

use crate::error::BentoDeskError;

/// Information about a single file on the desktop.
#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub file_type: String,
    pub modified_at: String,
    pub created_at: String,
    pub is_directory: bool,
    pub extension: Option<String>,
}

/// Scan the desktop directory and return metadata for each entry.
pub fn scan_desktop_files(desktop_path: &Path) -> Result<Vec<FileInfo>, BentoDeskError> {
    let mut files = Vec::new();

    if !desktop_path.exists() {
        return Ok(files);
    }

    for entry in std::fs::read_dir(desktop_path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let path = entry.path();

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Skip hidden files (starting with '.')
        if name.starts_with('.') {
            continue;
        }

        let extension = path
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
            .and_then(|t| {
                let datetime: chrono::DateTime<chrono::Utc> = t.into();
                Some(datetime.to_rfc3339())
            })
            .unwrap_or_default();

        let created_at = metadata
            .created()
            .ok()
            .and_then(|t| {
                let datetime: chrono::DateTime<chrono::Utc> = t.into();
                Some(datetime.to_rfc3339())
            })
            .unwrap_or_default();

        files.push(FileInfo {
            name,
            path: path.to_string_lossy().to_string(),
            size: metadata.len(),
            file_type,
            modified_at,
            created_at,
            is_directory: metadata.is_dir(),
            extension,
        });
    }

    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(files)
}
