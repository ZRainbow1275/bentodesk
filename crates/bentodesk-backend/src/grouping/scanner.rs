//! T-087 — desktop file scanner.
//!
//! Reads the user's Desktop directory and returns metadata for every file
//! and folder present, which the rules engine and suggestion engine
//! consume.
//!
//! Differences vs 1.x:
//! - `chrono::DateTime::<Utc>::from(SystemTime).to_rfc3339()` replaced by
//!   [`crate::time::system_time_to_rfc3339`] (Q1 ruling).
//! - `crate::error::BentoDeskError` replaced by hand-rolled
//!   [`ScannerError`] (spec §8.1).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::time;

/// Errors surfaced by the scanner.
#[derive(Debug)]
pub enum ScannerError {
    /// `read_dir` / `entry.metadata()` failed.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl core::fmt::Display for ScannerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "scanner I/O failed for {}: {source}", path.display())
            }
        }
    }
}

impl core::error::Error for ScannerError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
        }
    }
}

/// Information about a single file on the desktop.
///
/// `path`/`name`/`extension`/`file_type` use [`String`] because Windows
/// paths and file names routinely exceed the `SmolStr` 22-byte inline budget
/// (spec §10).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
///
/// Returns `Ok(Vec::new())` when `desktop_path` does not exist (matches the
/// 1.x first-launch behaviour). Bubbles up the *first* I/O error otherwise
/// — the rules engine treats a hard error as "scan unavailable, skip
/// evaluation" via the caller's `unwrap_or_default()`.
pub fn scan_desktop_files(desktop_path: &Path) -> Result<Vec<FileInfo>, ScannerError> {
    let mut files = Vec::new();
    if !desktop_path.exists() {
        return Ok(files);
    }

    let read_dir = std::fs::read_dir(desktop_path).map_err(|e| ScannerError::Io {
        path: desktop_path.to_path_buf(),
        source: e,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|e| ScannerError::Io {
            path: desktop_path.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        let metadata = entry.metadata().map_err(|e| ScannerError::Io {
            path: path.clone(),
            source: e,
        })?;

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

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
            .map(time::system_time_to_rfc3339)
            .unwrap_or_default();
        let created_at = metadata
            .created()
            .map(time::system_time_to_rfc3339)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn scratch_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let tid = std::thread::current().id();
        let path = std::env::temp_dir().join(format!("bentodesk-scanner-{tid:?}-{n}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    #[test]
    fn scan_nonexistent_returns_empty() {
        let dir = scratch_dir().join("nope");
        let files = scan_desktop_files(&dir).expect("scan");
        assert!(files.is_empty());
    }

    #[test]
    fn scan_returns_sorted_metadata() {
        let dir = scratch_dir();
        std::fs::write(dir.join("zebra.txt"), b"z").expect("write zebra");
        std::fs::write(dir.join("apple.md"), b"a").expect("write apple");
        let files = scan_desktop_files(&dir).expect("scan");
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["apple.md", "zebra.txt"]);
        assert_eq!(files[0].extension.as_deref(), Some("md"));
        assert_eq!(files[1].size, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_skips_dotfiles() {
        let dir = scratch_dir();
        std::fs::write(dir.join("visible.txt"), b"v").expect("write visible");
        std::fs::write(dir.join(".hidden"), b"h").expect("write hidden");
        let files = scan_desktop_files(&dir).expect("scan");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "visible.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fileinfo_serde_round_trip() {
        let f = FileInfo {
            name: "x.txt".into(),
            path: "C:/x.txt".into(),
            size: 12,
            file_type: "txt".into(),
            modified_at: "2026-01-01T00:00:00Z".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            is_directory: false,
            extension: Some("txt".into()),
        };
        let json = serde_json::to_string(&f).expect("ser");
        let back: FileInfo = serde_json::from_str(&json).expect("de");
        assert_eq!(back, f);
    }
}
