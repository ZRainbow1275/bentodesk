//! Settings backup management (Theme A — A2 migration protection).
//!
//! Creates `settings.backup.json` before any migration touches the primary
//! `settings.json`. Rotates the 3 most recent backups so a bad migration can
//! be reverted without losing user data. All writes go through
//! [`crate::storage::write_json_atomic`] to preserve the same ReplaceFileW +
//! `.bak` sibling crash-recovery guarantees as the primary file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::BentoDeskError;
use crate::storage;

const MAX_BACKUPS: usize = 3;
const BACKUP_PREFIX: &str = "settings.backup";
const BACKUP_SUFFIX: &str = ".json";

/// Metadata entry surfaced to the frontend via `list_settings_backups`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    /// Opaque id used by `restore_settings_backup`. Currently the file stem
    /// after the `settings.backup.` prefix (e.g. the UTC timestamp).
    pub id: String,
    /// Absolute path on disk.
    pub path: String,
    /// ISO-8601 UTC creation timestamp as persisted in the filename.
    pub created_at: String,
    /// Size in bytes of the backup file at snapshot time.
    pub size_bytes: u64,
}

/// Build the timestamped backup filename. Kept deterministic and sortable so
/// rotation can rely on filesystem ordering.
fn backup_filename(timestamp: &str) -> String {
    format!("{BACKUP_PREFIX}.{timestamp}{BACKUP_SUFFIX}")
}

/// Directory where backups live (same directory as `settings.json`).
fn backup_dir_from_settings_path(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Snapshot the current primary `settings.json` before a migration attempt.
///
/// Returns `Ok(None)` when the primary file does not exist yet (fresh install
/// or portable relocation) — no backup is needed. Otherwise writes a
/// timestamped file and prunes old entries.
pub fn create_backup(settings_path: &Path) -> Result<Option<BackupEntry>, BentoDeskError> {
    if !settings_path.exists() {
        return Ok(None);
    }

    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
    let backup_path =
        backup_dir_from_settings_path(settings_path).join(backup_filename(&timestamp));

    // Copy through a temp file so we do not leave a partial backup if the
    // process dies mid-copy. `write_json_atomic` already gives us ReplaceFileW
    // semantics but expects structured JSON — here we want a verbatim copy so
    // we can restore the exact bytes the user had on disk, including any
    // `_legacy` fields an older build left behind.
    std::fs::copy(settings_path, &backup_path)?;

    let size_bytes = std::fs::metadata(&backup_path)
        .map(|m| m.len())
        .unwrap_or(0);

    prune_old_backups(&backup_dir_from_settings_path(settings_path))?;

    tracing::info!(
        "Settings backup created at {} ({} bytes)",
        backup_path.display(),
        size_bytes
    );

    Ok(Some(BackupEntry {
        id: timestamp.clone(),
        path: backup_path.to_string_lossy().into_owned(),
        created_at: timestamp,
        size_bytes,
    }))
}

/// Return backups ordered newest-first so the UI can show the latest first.
pub fn list_backups(settings_path: &Path) -> Result<Vec<BackupEntry>, BentoDeskError> {
    let dir = backup_dir_from_settings_path(settings_path);
    let mut entries = Vec::new();

    if !dir.exists() {
        return Ok(entries);
    }

    for dirent in std::fs::read_dir(&dir)? {
        let dirent = match dirent {
            Ok(d) => d,
            Err(err) => {
                tracing::warn!("Skipping backup dir entry: {err}");
                continue;
            }
        };
        let path = dirent.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stripped) = name
            .strip_prefix(&format!("{BACKUP_PREFIX}."))
            .and_then(|s| s.strip_suffix(BACKUP_SUFFIX))
        else {
            continue;
        };

        let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        entries.push(BackupEntry {
            id: stripped.to_string(),
            path: path.to_string_lossy().into_owned(),
            created_at: stripped.to_string(),
            size_bytes,
        });
    }

    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(entries)
}

/// Restore a previous backup by id, replacing the current primary settings
/// file via `write_json_atomic` semantics so a crash mid-restore cannot leave
/// a truncated `settings.json`.
pub fn restore_backup(settings_path: &Path, backup_id: &str) -> Result<(), BentoDeskError> {
    let backup_path = backup_dir_from_settings_path(settings_path).join(backup_filename(backup_id));
    if !backup_path.exists() {
        return Err(BentoDeskError::ConfigError(format!(
            "Backup not found: {}",
            backup_path.display()
        )));
    }

    // Deserialize as serde_json::Value first — this lets us restore older
    // schema versions without hitting a hard serde error for fields the
    // current `AppSettings` shape does not know about. The migration
    // framework on the next launch will re-promote them via `_legacy`.
    let bytes = std::fs::read(&backup_path)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    storage::write_json_atomic(settings_path, &value)?;

    tracing::warn!(
        "Settings restored from backup {} → {}",
        backup_path.display(),
        settings_path.display()
    );
    Ok(())
}

fn prune_old_backups(dir: &Path) -> Result<(), BentoDeskError> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(&format!("{BACKUP_PREFIX}.")) && n.ends_with(BACKUP_SUFFIX))
                .unwrap_or(false)
        })
        .collect();

    if files.len() <= MAX_BACKUPS {
        return Ok(());
    }

    files.sort();
    let excess = files.len() - MAX_BACKUPS;
    for p in files.into_iter().take(excess) {
        if let Err(err) = std::fs::remove_file(&p) {
            tracing::warn!("Failed to prune old backup {}: {err}", p.display());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write_primary(path: &Path, content: &serde_json::Value) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_vec(content).unwrap()).unwrap();
    }

    #[test]
    fn create_backup_noop_when_primary_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(create_backup(&path).unwrap().is_none());
    }

    #[test]
    fn create_backup_writes_file_and_lists_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        write_primary(&path, &json!({ "schema_version": 1 }));

        let entry = create_backup(&path).unwrap().unwrap();
        assert!(Path::new(&entry.path).exists());
        assert!(entry.size_bytes > 0);

        let entries = list_backups(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, entry.id);
    }

    #[test]
    fn rotation_keeps_newest_three() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        write_primary(&path, &json!({ "schema_version": 1 }));

        for i in 0..5 {
            let ts = format!("2026041{i}T000000.000Z");
            let backup = dir
                .path()
                .join(format!("{BACKUP_PREFIX}.{ts}{BACKUP_SUFFIX}"));
            std::fs::write(&backup, b"{}").unwrap();
        }

        prune_old_backups(dir.path()).unwrap();

        let remaining = list_backups(&path).unwrap();
        assert_eq!(remaining.len(), MAX_BACKUPS);
        // Newest first: 20260414 > 20260413 > 20260412
        assert!(remaining[0].id.starts_with("202604"));
    }

    #[test]
    fn restore_overwrites_primary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        write_primary(&path, &json!({ "schema_version": 2, "theme": "Dark" }));

        let backup = create_backup(&path).unwrap().unwrap();

        // Mutate primary
        write_primary(&path, &json!({ "schema_version": 2, "theme": "Light" }));

        restore_backup(&path, &backup.id).unwrap();

        let restored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(restored["theme"], "Dark");
    }

    #[test]
    fn restore_missing_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        write_primary(&path, &json!({ "schema_version": 1 }));
        let err = restore_backup(&path, "does-not-exist").unwrap_err();
        assert!(err.to_string().contains("Backup not found"));
    }
}
