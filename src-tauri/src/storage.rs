//! Atomic JSON persistence helpers with backup recovery.
//!
//! Persisted BentoDesk state is written to a same-directory temporary file,
//! flushed to disk, and then swapped into place. The previous primary file is
//! retained as a `.bak` sibling so startup can recover from truncated or
//! otherwise corrupt JSON after crashes or interrupted writes.

use serde::{de::DeserializeOwned, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::BentoDeskError;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

pub(crate) const MAX_JSON_STATE_BYTES: u64 = 128 * 1024 * 1024;
const JSON_STATE_LIMIT_ERROR_MESSAGE: &str = "json_state_limit_exceeded";

/// Return the backup file path used for the given JSON file.
pub fn backup_path(path: &Path) -> PathBuf {
    sibling_path_with_suffix(path, ".bak")
}

/// Atomically write JSON to disk, keeping the previous primary file as backup.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), BentoDeskError> {
    write_json_atomic_with_limit(path, value, MAX_JSON_STATE_BYTES)
}

fn write_json_atomic_with_limit<T: Serialize>(
    path: &Path,
    value: &T,
    max_bytes: u64,
) -> Result<(), BentoDeskError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let temp_path = sibling_path_with_suffix(path, ".tmp");
    if temp_path.exists() {
        let _ = std::fs::remove_file(&temp_path);
    }

    let prepare_result = (|| {
        let mut temp_file = File::create(&temp_path)?;
        write_json_to_writer_with_limit(&mut temp_file, path, value, max_bytes)?;
        temp_file.sync_all()?;
        Ok::<(), BentoDeskError>(())
    })();

    if let Err(err) = prepare_result {
        let _ = std::fs::remove_file(&temp_path);
        return Err(err);
    }

    let backup = backup_path(path);
    let replace_result = replace_file_with_backup(&temp_path, path, &backup);
    if replace_result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    replace_result
}

fn write_json_to_writer_with_limit<T: Serialize, W: Write>(
    writer: &mut W,
    path: &Path,
    value: &T,
    max_bytes: u64,
) -> Result<(), BentoDeskError> {
    let mut limited_writer = LimitedWriter::new(writer, max_bytes);
    if let Err(err) = serde_json::to_writer(&mut limited_writer, value) {
        if let Some(attempted_bytes) = limited_writer.limit_exceeded_bytes() {
            return Err(BentoDeskError::Generic(json_state_limit_message(
                path,
                attempted_bytes,
                max_bytes,
            )));
        }

        return Err(err.into());
    }

    Ok(())
}

/// Read JSON from disk, automatically falling back to the `.bak` file when the
/// primary file is missing or corrupt.
pub fn read_json_with_recovery<T>(path: &Path, label: &str) -> Result<Option<T>, BentoDeskError>
where
    T: DeserializeOwned + Serialize,
{
    match read_json_file(path) {
        Ok(Some(value)) => Ok(Some(value)),
        Ok(None) => recover_from_backup(path, label, None),
        Err(primary_err) => recover_from_backup(path, label, Some(primary_err)),
    }
}

fn read_json_file<T>(path: &Path) -> Result<Option<T>, BentoDeskError>
where
    T: DeserializeOwned,
{
    read_json_file_with_limit(path, MAX_JSON_STATE_BYTES)
}

fn read_json_file_with_limit<T>(path: &Path, max_bytes: u64) -> Result<Option<T>, BentoDeskError>
where
    T: DeserializeOwned,
{
    if !path.exists() {
        return Ok(None);
    }

    let file_size = std::fs::metadata(path)?.len();
    if file_size > max_bytes {
        return Err(BentoDeskError::Generic(json_state_limit_message(
            path, file_size, max_bytes,
        )));
    }

    let bytes = std::fs::read(path)?;
    let value = serde_json::from_slice::<T>(&bytes)?;
    Ok(Some(value))
}

fn recover_from_backup<T>(
    path: &Path,
    label: &str,
    primary_error: Option<BentoDeskError>,
) -> Result<Option<T>, BentoDeskError>
where
    T: DeserializeOwned + Serialize,
{
    let backup = backup_path(path);
    if !backup.exists() {
        return match primary_error {
            Some(err) => Err(err),
            None => Ok(None),
        };
    }

    let recovered = match read_json_file::<T>(&backup) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return Err(BentoDeskError::Generic(format!(
                "{label} backup missing: {}",
                backup.display()
            )))
        }
        Err(backup_err) => {
            let primary_text = primary_error
                .as_ref()
                .map_or_else(|| "primary file missing".to_string(), ToString::to_string);
            return Err(BentoDeskError::Generic(format!(
                "{label} primary/backup recovery failed at {}: {primary_text}; backup: {backup_err}",
                path.display()
            )));
        }
    };

    if path.exists() {
        quarantine_corrupt_file(path);
    }

    match write_json_atomic(path, &recovered) {
        Ok(()) => {
            tracing::warn!(
                path = %path.display(),
                backup = %backup.display(),
                "{label} recovered from backup and primary file was healed"
            );
        }
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                backup = %backup.display(),
                error = %err,
                "{label} recovered from backup but primary rewrite failed"
            );
        }
    }

    Ok(Some(recovered))
}

fn quarantine_corrupt_file(path: &Path) {
    let corrupt_path = sibling_path_with_suffix(
        path,
        &format!(
            ".corrupt-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ")
        ),
    );

    match std::fs::rename(path, &corrupt_path) {
        Ok(()) => {
            tracing::warn!(
                path = %path.display(),
                quarantined = %corrupt_path.display(),
                "Quarantined unreadable JSON file"
            );
        }
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                quarantined = %corrupt_path.display(),
                error = %err,
                "Failed to quarantine unreadable JSON file before recovery"
            );
        }
    }
}

fn sibling_path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".to_string());
    path.with_file_name(format!("{file_name}{suffix}"))
}

fn json_state_limit_message(path: &Path, bytes: u64, max_bytes: u64) -> String {
    format!(
        "JSON state file exceeds the safety limit at {}: {} bytes > {} bytes",
        path.display(),
        bytes,
        max_bytes
    )
}

struct LimitedWriter<W> {
    inner: W,
    written: u64,
    max_bytes: u64,
    limit_exceeded_bytes: Option<u64>,
}

impl<W> LimitedWriter<W> {
    fn new(inner: W, max_bytes: u64) -> Self {
        Self {
            inner,
            written: 0,
            max_bytes,
            limit_exceeded_bytes: None,
        }
    }

    fn limit_exceeded_bytes(&self) -> Option<u64> {
        self.limit_exceeded_bytes
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let attempted_bytes = self.written.saturating_add(buf.len() as u64);
        if attempted_bytes > self.max_bytes {
            self.limit_exceeded_bytes = Some(attempted_bytes);
            return Err(std::io::Error::other(JSON_STATE_LIMIT_ERROR_MESSAGE));
        }

        let written = self.inner.write(buf)?;
        self.written = self.written.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(windows)]
fn replace_file_with_backup(
    temp_path: &Path,
    path: &Path,
    backup_path: &Path,
) -> Result<(), BentoDeskError> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        REPLACEFILE_IGNORE_MERGE_ERRORS,
    };

    let temp_w = to_wide_path(temp_path);
    let path_w = to_wide_path(path);
    let backup_w = to_wide_path(backup_path);

    let result = if path.exists() {
        // SAFETY: All three paths are valid, null-terminated UTF-16 strings that
        // live for the duration of the call. The replacement file is fully
        // written and flushed before ReplaceFileW is invoked.
        unsafe {
            ReplaceFileW(
                PCWSTR(path_w.as_ptr()),
                PCWSTR(temp_w.as_ptr()),
                PCWSTR(backup_w.as_ptr()),
                REPLACEFILE_IGNORE_MERGE_ERRORS,
                None,
                None,
            )
        }
    } else {
        // SAFETY: Both paths are valid, null-terminated UTF-16 strings that
        // live for the duration of the call. MOVEFILE_WRITE_THROUGH ensures the
        // rename is flushed before the API returns.
        unsafe {
            MoveFileExW(
                PCWSTR(temp_w.as_ptr()),
                PCWSTR(path_w.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
    };

    result.map_err(|err| {
        BentoDeskError::Generic(format!("Atomic write failed for {}: {err}", path.display()))
    })
}

#[cfg(not(windows))]
fn replace_file_with_backup(
    temp_path: &Path,
    path: &Path,
    backup_path: &Path,
) -> Result<(), BentoDeskError> {
    if path.exists() {
        std::fs::copy(path, backup_path)?;
    }
    std::fs::rename(temp_path, path)?;
    Ok(())
}

#[cfg(windows)]
fn to_wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct TestData {
        name: String,
        count: u32,
    }

    #[test]
    fn atomic_write_creates_backup_on_replace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        write_json_atomic(
            &path,
            &TestData {
                name: "first".to_string(),
                count: 1,
            },
        )
        .unwrap();
        assert!(path.exists());
        assert!(!backup_path(&path).exists());

        write_json_atomic(
            &path,
            &TestData {
                name: "second".to_string(),
                count: 2,
            },
        )
        .unwrap();

        let current: TestData =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let backup: TestData =
            serde_json::from_str(&std::fs::read_to_string(backup_path(&path)).unwrap()).unwrap();

        assert_eq!(
            current,
            TestData {
                name: "second".to_string(),
                count: 2,
            }
        );
        assert_eq!(
            backup,
            TestData {
                name: "first".to_string(),
                count: 1,
            }
        );
    }

    #[test]
    fn read_json_recovers_from_backup_and_rewrites_primary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        write_json_atomic(
            &path,
            &TestData {
                name: "first".to_string(),
                count: 1,
            },
        )
        .unwrap();
        write_json_atomic(
            &path,
            &TestData {
                name: "second".to_string(),
                count: 2,
            },
        )
        .unwrap();

        std::fs::write(&path, "{ not valid json").unwrap();

        let recovered = read_json_with_recovery::<TestData>(&path, "Test state")
            .unwrap()
            .unwrap();
        assert_eq!(
            recovered,
            TestData {
                name: "first".to_string(),
                count: 1,
            }
        );

        let healed: TestData =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(healed, recovered);
    }

    #[test]
    fn oversized_json_is_rejected_before_parse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        std::fs::write(&path, b"{\"name\":\"ok\",\"count\":1}").unwrap();
        let result = read_json_file_with_limit::<TestData>(&path, 8).unwrap_err();

        assert!(result
            .to_string()
            .contains("JSON state file exceeds the safety limit"));
    }

    #[test]
    fn oversized_json_write_is_rejected_before_replace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let stable = TestData {
            name: "stable".to_string(),
            count: 1,
        };

        write_json_atomic(&path, &stable).unwrap();

        let err = write_json_atomic_with_limit(
            &path,
            &TestData {
                name: "x".repeat(128),
                count: 2,
            },
            48,
        )
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("JSON state file exceeds the safety limit"));

        let current: TestData =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(current, stable);
        assert!(!sibling_path_with_suffix(&path, ".tmp").exists());
    }
}
