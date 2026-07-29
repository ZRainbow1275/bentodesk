//! T-090 — Atomic JSON persistence helpers (lift-port from 1.x
//! `src-tauri/src/storage.rs`).
//!
//! Persisted BentoDesk state is written to a same-directory temporary file,
//! flushed to disk, and then swapped into place via `ReplaceFileW`
//! (Windows) / `rename` (other). The previous primary file is retained as
//! a `.bak` sibling so startup can recover from truncated or otherwise
//! corrupt JSON after crashes or interrupted writes.
//!
//! # What changed vs 1.x
//!
//! | 1.x                                                           | native                                                                   |
//! |---------------------------------------------------------------|------------------------------------------------------------------------|
//! | `tauri::AppHandle` for `app_data_dir`                         | caller passes `&Path` (or uses [`fallback_state_data_dir`])            |
//! | `dirs::data_dir()`                                            | `SHGetKnownFolderPath(FOLDERID_RoamingAppData)` via Win32 directly     |
//! | `chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ")`             | [`crate::time::now_compact_rfc3339`] — hand-rolled, no chrono dep      |
//! | `BentoDeskError::Generic` / `BentoDeskError::Io`              | hand-rolled [`StorageError`] enum (spec §8.1, no thiserror)            |

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

/// Maximum size of a single JSON state file. Anything larger is rejected at
/// read or write time as a safety check (a runaway write should not eat all
/// the user's disk).
pub const MAX_JSON_STATE_BYTES: u64 = 128 * 1024 * 1024;

const JSON_STATE_LIMIT_ERROR_MESSAGE: &str = "json_state_limit_exceeded";

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the atomic-storage helpers.
#[derive(Debug)]
pub enum StorageError {
    /// Filesystem I/O (`std::fs::*`) failed.
    Io { path: PathBuf, message: String },
    /// JSON serialize / deserialize failed.
    Json { path: PathBuf, message: String },
    /// File exceeds the [`MAX_JSON_STATE_BYTES`] safety limit.
    LimitExceeded {
        path: PathBuf,
        bytes: u64,
        max_bytes: u64,
    },
    /// `ReplaceFileW` / `MoveFileExW` (Windows) returned a non-`S_OK` HRESULT.
    Replace { path: PathBuf, message: String },
    /// `read_json_with_recovery` exhausted both primary and backup.
    Recovery { path: PathBuf, message: String },
    /// `SHGetKnownFolderPath` did not return `S_OK`.
    KnownFolder { ctx: &'static str, hr: i32 },
    /// `SHGetKnownFolderPath` reported success without returning a path.
    NullKnownFolder { ctx: &'static str },
}

impl core::fmt::Display for StorageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(f, "storage io error at {}: {}", path.display(), message)
            }
            Self::Json { path, message } => {
                write!(f, "storage json error at {}: {}", path.display(), message)
            }
            Self::LimitExceeded {
                path,
                bytes,
                max_bytes,
            } => write!(
                f,
                "JSON state file exceeds the safety limit at {}: {} bytes > {} bytes",
                path.display(),
                bytes,
                max_bytes
            ),
            Self::Replace { path, message } => {
                write!(f, "atomic write failed for {}: {}", path.display(), message)
            }
            Self::Recovery { path, message } => write!(
                f,
                "primary/backup recovery failed at {}: {}",
                path.display(),
                message
            ),
            Self::KnownFolder { ctx, hr } => {
                write!(f, "{ctx}: SHGetKnownFolderPath failed (hr={hr:#x})")
            }
            Self::NullKnownFolder { ctx } => {
                write!(f, "{ctx}: SHGetKnownFolderPath returned a null path")
            }
        }
    }
}

impl core::error::Error for StorageError {}

// ─── State directory resolution ──────────────────────────────────────

/// Resolve the shared BentoDesk state directory.
///
/// Rules:
/// - Prefer `./data` beside the executable when running in portable mode.
/// - Otherwise use the platform's roaming app-data directory.
/// - In debug/dev builds, suffix the directory so dev sessions do not
///   mutate the installed release's persisted state.
pub fn state_data_dir() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        let portable_dir = exe_path.parent().map(|p| p.join("data"));
        if let Some(ref dir) = portable_dir {
            if dir.exists() {
                return dir.clone();
            }
        }
    }

    isolate_debug_data_dir(fallback_state_data_dir())
}

/// Fallback state directory.
///
/// Resolves `%APPDATA%\BentoDesk` (or `BentoDesk-Dev` in debug builds).
/// On non-Windows we fall back to the current directory — the native backend
/// is Windows-only by spec but the cross-cfg guard keeps the type signatures
/// honest for tests.
pub fn fallback_state_data_dir() -> PathBuf {
    let folder = if cfg!(debug_assertions) {
        "BentoDesk-Dev"
    } else {
        "BentoDesk"
    };

    #[cfg(windows)]
    {
        match known_folder_roaming_appdata() {
            Ok(base) => base.join(folder),
            Err(err) => {
                tracing::warn!("SHGetKnownFolderPath failed, using cwd fallback: {err}");
                PathBuf::from(".").join(folder)
            }
        }
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(".").join(folder)
    }
}

fn isolate_debug_data_dir(base: PathBuf) -> PathBuf {
    if !cfg!(debug_assertions) {
        return base;
    }

    let Some(name) = base.file_name().and_then(|part| part.to_str()) else {
        return base.join("dev");
    };
    base.with_file_name(format!("{name}-dev"))
}

/// `SHGetKnownFolderPath(FOLDERID_RoamingAppData)` → `%APPDATA%`.
#[cfg(windows)]
fn known_folder_roaming_appdata() -> Result<PathBuf, StorageError> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{
        FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
    };

    // SAFETY: SHGetKnownFolderPath writes a COM-allocated PWSTR into our
    // out-pointer. We free it via CoTaskMemFree below regardless of the
    // result. `None` for the token argument requests the current user.
    let pwstr = unsafe { SHGetKnownFolderPath(&FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, None) };
    let pwstr = match pwstr {
        Ok(p) => p,
        Err(e) => {
            return Err(StorageError::KnownFolder {
                ctx: "FOLDERID_RoamingAppData",
                hr: e.code().0,
            });
        }
    };
    if pwstr.as_ptr().is_null() {
        return Err(StorageError::NullKnownFolder {
            ctx: "FOLDERID_RoamingAppData",
        });
    }

    // SAFETY: pwstr is a COM-allocated null-terminated UTF-16 string; convert
    // to a Rust String, then free via CoTaskMemFree to balance the COM alloc.
    let path_str = unsafe { pwstr.to_string() };
    // SAFETY: pwstr was returned by SHGetKnownFolderPath; CoTaskMemFree is the
    // documented disposal call.
    unsafe { CoTaskMemFree(Some(pwstr.as_ptr() as *const _)) };

    let path = path_str.map_err(|_| StorageError::KnownFolder {
        ctx: "PWSTR::to_string",
        hr: 0,
    })?;
    Ok(PathBuf::from(path))
}

// ─── Path helpers ────────────────────────────────────────────────────

/// Return the backup file path used for the given JSON file
/// (`{path}.bak`).
pub fn backup_path(path: &Path) -> PathBuf {
    sibling_path_with_suffix(path, ".bak")
}

fn sibling_path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".to_string());
    path.with_file_name(format!("{file_name}{suffix}"))
}

// ─── Atomic write ────────────────────────────────────────────────────

/// Atomically write JSON to disk, keeping the previous primary file as
/// backup. Bounded by [`MAX_JSON_STATE_BYTES`].
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), StorageError> {
    write_json_atomic_with_limit(path, value, MAX_JSON_STATE_BYTES)
}

fn write_json_atomic_with_limit<T: Serialize>(
    path: &Path,
    value: &T,
    max_bytes: u64,
) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StorageError::Io {
            path: parent.to_path_buf(),
            message: e.to_string(),
        })?;
    }

    let temp_path = sibling_path_with_suffix(path, ".tmp");
    if temp_path.exists() {
        let _ = std::fs::remove_file(&temp_path);
    }

    let prepare_result = (|| -> Result<(), StorageError> {
        let mut temp_file = File::create(&temp_path).map_err(|e| StorageError::Io {
            path: temp_path.clone(),
            message: e.to_string(),
        })?;
        write_json_to_writer_with_limit(&mut temp_file, path, value, max_bytes)?;
        temp_file.sync_all().map_err(|e| StorageError::Io {
            path: temp_path.clone(),
            message: e.to_string(),
        })?;
        Ok(())
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
) -> Result<(), StorageError> {
    // `serde_json::to_writer` requires the `std` feature on `serde_json`,
    // which the workspace dep deliberately does not enable (spec §8 — keep
    // the feature surface small). Round-trip through a `Vec<u8>` instead;
    // the typical state file is ≤128 KiB so the heap detour is cheap.
    let bytes = serde_json::to_vec(value).map_err(|err| StorageError::Json {
        path: path.to_path_buf(),
        message: err.to_string(),
    })?;
    let total = bytes.len() as u64;
    if total > max_bytes {
        return Err(StorageError::LimitExceeded {
            path: path.to_path_buf(),
            bytes: total,
            max_bytes,
        });
    }
    let mut limited_writer = LimitedWriter::new(writer, max_bytes);
    if let Err(err) = limited_writer.write_all(&bytes) {
        if let Some(attempted_bytes) = limited_writer.limit_exceeded_bytes() {
            return Err(StorageError::LimitExceeded {
                path: path.to_path_buf(),
                bytes: attempted_bytes,
                max_bytes,
            });
        }
        return Err(StorageError::Io {
            path: path.to_path_buf(),
            message: err.to_string(),
        });
    }
    Ok(())
}

// ─── Read with backup recovery ───────────────────────────────────────

/// Read JSON from disk, automatically falling back to the `.bak` file when
/// the primary file is missing or corrupt. On successful recovery the
/// primary file is healed with the recovered value.
pub fn read_json_with_recovery<T>(path: &Path, label: &str) -> Result<Option<T>, StorageError>
where
    T: DeserializeOwned + Serialize,
{
    match read_json_file(path) {
        Ok(Some(value)) => Ok(Some(value)),
        Ok(None) => recover_from_backup(path, label, None),
        Err(primary_err) => recover_from_backup(path, label, Some(primary_err)),
    }
}

fn read_json_file<T>(path: &Path) -> Result<Option<T>, StorageError>
where
    T: DeserializeOwned,
{
    read_json_file_with_limit(path, MAX_JSON_STATE_BYTES)
}

fn read_json_file_with_limit<T>(path: &Path, max_bytes: u64) -> Result<Option<T>, StorageError>
where
    T: DeserializeOwned,
{
    if !path.exists() {
        return Ok(None);
    }

    let metadata = std::fs::metadata(path).map_err(|e| StorageError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    let file_size = metadata.len();
    if file_size > max_bytes {
        return Err(StorageError::LimitExceeded {
            path: path.to_path_buf(),
            bytes: file_size,
            max_bytes,
        });
    }

    let bytes = std::fs::read(path).map_err(|e| StorageError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    let value = serde_json::from_slice::<T>(&bytes).map_err(|e| StorageError::Json {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    Ok(Some(value))
}

fn recover_from_backup<T>(
    path: &Path,
    label: &str,
    primary_error: Option<StorageError>,
) -> Result<Option<T>, StorageError>
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
            return Err(StorageError::Recovery {
                path: path.to_path_buf(),
                message: format!("{label} backup missing: {}", backup.display()),
            });
        }
        Err(backup_err) => {
            let primary_text = primary_error
                .as_ref()
                .map_or_else(|| "primary file missing".to_string(), ToString::to_string);
            return Err(StorageError::Recovery {
                path: path.to_path_buf(),
                message: format!("{label}: {primary_text}; backup: {backup_err}"),
            });
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
                "{label} recovered from backup and primary file was healed",
            );
        }
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                backup = %backup.display(),
                error = %err,
                "{label} recovered from backup but primary rewrite failed",
            );
        }
    }

    Ok(Some(recovered))
}

fn quarantine_corrupt_file(path: &Path) {
    let stamp = crate::time::now_compact_rfc3339();
    let corrupt_path = sibling_path_with_suffix(path, &format!(".corrupt-{stamp}"));

    match std::fs::rename(path, &corrupt_path) {
        Ok(()) => {
            tracing::warn!(
                path = %path.display(),
                quarantined = %corrupt_path.display(),
                "Quarantined unreadable JSON file",
            );
        }
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                quarantined = %corrupt_path.display(),
                error = %err,
                "Failed to quarantine unreadable JSON file before recovery",
            );
        }
    }
}

// ─── Bounded-write helper ────────────────────────────────────────────

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

        let written = self.inner.write(buf).map_err(std::io::Error::other)?;
        self.written = self.written.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

// ─── Atomic replace (Win32 ReplaceFileW / MoveFileExW) ───────────────

#[cfg(windows)]
fn replace_file_with_backup(
    temp_path: &Path,
    path: &Path,
    backup_path: &Path,
) -> Result<(), StorageError> {
    use windows::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW,
    };
    use windows::core::PCWSTR;

    let temp_w = to_wide_path(temp_path);
    let path_w = to_wide_path(path);
    let backup_w = to_wide_path(backup_path);

    // The `path.exists()` probe is only a fast-path selector, never
    // load-bearing for correctness:
    //
    //   * On the common NTFS path the destination exists, so we try
    //     `ReplaceFileW` first — it gives us an atomic swap *and* writes the
    //     prior contents into the `.bak` sibling in one call.
    //   * If `ReplaceFileW` returns `Err`, we do NOT treat it as fatal. It
    //     can fail on FAT32 / exotic filesystems (USB sticks, SD cards, some
    //     network shares) that lack the features it relies on — e.g.
    //     ERROR_UNABLE_TO_REMOVE_REPLACED / ERROR_UNABLE_TO_MOVE_REPLACEMENT
    //     — and it can also lose a TOCTOU race if the destination is deleted
    //     between the `exists()` probe and the call (ERROR_FILE_NOT_FOUND).
    //     In every such case we fall through to the `MoveFileExW` last
    //     resort so the freshly written `.tmp` is never stranded and the
    //     user's edits are not silently lost.
    //   * When the destination is missing there is nothing for
    //     `ReplaceFileW` to back up, so we go straight to `MoveFileExW`.
    //
    // `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` is the
    // robust fallback: it creates-or-replaces and works on FAT32 (atomic
    // rename on NTFS, delete+rename on FAT32), flushing before it returns.
    if path.exists() {
        // SAFETY: All three paths are valid, null-terminated UTF-16 strings
        // that live for the duration of the call. The replacement file is
        // fully written and flushed before ReplaceFileW is invoked.
        let replace_result = unsafe {
            ReplaceFileW(
                PCWSTR(path_w.as_ptr()),
                PCWSTR(temp_w.as_ptr()),
                PCWSTR(backup_w.as_ptr()),
                REPLACEFILE_IGNORE_MERGE_ERRORS,
                None,
                None,
            )
        };
        if replace_result.is_ok() {
            return Ok(());
        }
        // ReplaceFileW failed (FAT32 / merge / unable-to-remove / TOCTOU) —
        // fall through to the MoveFileExW fallback below rather than
        // surfacing a fatal error and stranding the write in `.tmp`.
    }

    // Fallback path (dest missing, or ReplaceFileW failed). Best-effort
    // preserve the prior primary as a backup, mirroring the non-Windows arm;
    // a failed backup copy must NOT fail the write (backup is best-effort).
    if path.exists() {
        let _ = std::fs::copy(path, backup_path);
    }

    // SAFETY: Both paths are valid, null-terminated UTF-16 strings that live
    // for the duration of the call. MOVEFILE_WRITE_THROUGH ensures the rename
    // is flushed before the API returns.
    let move_result = unsafe {
        MoveFileExW(
            PCWSTR(temp_w.as_ptr()),
            PCWSTR(path_w.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    move_result.map_err(|err: windows::core::Error| StorageError::Replace {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
}

#[cfg(not(windows))]
fn replace_file_with_backup(
    temp_path: &Path,
    path: &Path,
    backup_path: &Path,
) -> Result<(), StorageError> {
    if path.exists() {
        std::fs::copy(path, backup_path).map_err(|e| StorageError::Io {
            path: backup_path.to_path_buf(),
            message: e.to_string(),
        })?;
    }
    std::fs::rename(temp_path, path).map_err(|e| StorageError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
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
    fn null_known_folder_error_is_explicit() {
        let error = StorageError::NullKnownFolder { ctx: "AppData" };
        assert_eq!(
            error.to_string(),
            "AppData: SHGetKnownFolderPath returned a null path"
        );
    }

    #[test]
    fn atomic_write_creates_backup_on_replace() {
        let dir = tempdir();
        let path = dir.join("state.json");

        write_json_atomic(
            &path,
            &TestData {
                name: "first".to_string(),
                count: 1,
            },
        )
        .expect("write 1");
        assert!(path.exists());
        assert!(!backup_path(&path).exists());

        write_json_atomic(
            &path,
            &TestData {
                name: "second".to_string(),
                count: 2,
            },
        )
        .expect("write 2");

        let current: TestData =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        let backup: TestData = serde_json::from_str(
            &std::fs::read_to_string(backup_path(&path)).expect("read backup"),
        )
        .expect("parse backup");

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
    fn atomic_write_roundtrips_through_both_replace_branches() {
        // First write targets a NON-EXISTENT dest → exercises the
        // MoveFileExW create branch (which the FAT32 ReplaceFileW-failure
        // fallback shares). Second write OVERWRITES it → exercises the
        // ReplaceFileW dest-exists branch. Both must round-trip.
        let dir = tempdir();
        let path = dir.join("state.json");
        assert!(!path.exists(), "precondition: dest must not exist");

        let created = TestData {
            name: "created".to_string(),
            count: 7,
        };
        write_json_atomic(&path, &created).expect("write to missing dest (MoveFileExW branch)");
        assert!(path.exists());
        let after_create: TestData =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(after_create, created);

        let overwritten = TestData {
            name: "overwritten".to_string(),
            count: 8,
        };
        write_json_atomic(&path, &overwritten)
            .expect("overwrite existing dest (ReplaceFileW branch)");
        let after_overwrite: TestData =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(after_overwrite, overwritten);

        // The overwrite must have preserved the prior primary as `.bak`.
        let backup: TestData = serde_json::from_str(
            &std::fs::read_to_string(backup_path(&path)).expect("read backup"),
        )
        .expect("parse backup");
        assert_eq!(backup, created);
    }

    #[test]
    fn fallback_state_data_dir_reflects_current_build_flavor() {
        let path_text = fallback_state_data_dir().to_string_lossy().to_string();
        if cfg!(debug_assertions) {
            assert!(path_text.contains("BentoDesk-Dev"), "got: {path_text}");
        } else {
            assert!(path_text.contains("BentoDesk"), "got: {path_text}");
        }
    }

    #[test]
    fn read_json_recovers_from_backup_and_rewrites_primary() {
        let dir = tempdir();
        let path = dir.join("state.json");

        write_json_atomic(
            &path,
            &TestData {
                name: "first".to_string(),
                count: 1,
            },
        )
        .expect("write 1");
        write_json_atomic(
            &path,
            &TestData {
                name: "second".to_string(),
                count: 2,
            },
        )
        .expect("write 2");

        std::fs::write(&path, "{ not valid json").expect("corrupt");

        let recovered = read_json_with_recovery::<TestData>(&path, "Test state")
            .expect("recover")
            .expect("some");
        assert_eq!(
            recovered,
            TestData {
                name: "first".to_string(),
                count: 1,
            }
        );

        let healed: TestData =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(healed, recovered);
    }

    #[test]
    fn oversized_json_is_rejected_before_parse() {
        let dir = tempdir();
        let path = dir.join("state.json");

        std::fs::write(&path, b"{\"name\":\"ok\",\"count\":1}").expect("write seed");
        let result = read_json_file_with_limit::<TestData>(&path, 8).expect_err("must reject");

        assert!(matches!(result, StorageError::LimitExceeded { .. }));
    }

    #[test]
    fn oversized_json_write_is_rejected_before_replace() {
        let dir = tempdir();
        let path = dir.join("state.json");
        let stable = TestData {
            name: "stable".to_string(),
            count: 1,
        };

        write_json_atomic(&path, &stable).expect("write stable");

        let err = write_json_atomic_with_limit(
            &path,
            &TestData {
                name: "x".repeat(128),
                count: 2,
            },
            48,
        )
        .expect_err("must reject");

        assert!(matches!(err, StorageError::LimitExceeded { .. }));

        let current: TestData =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(current, stable);
        assert!(!sibling_path_with_suffix(&path, ".tmp").exists());
    }

    #[test]
    fn backup_path_appends_dot_bak() {
        let p = backup_path(Path::new("C:\\state.json"));
        assert!(p.to_string_lossy().ends_with("state.json.bak"));
    }

    /// Per-process unique temp directory rooted under the OS temp dir.
    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("bentodesk-storage-{pid}-{n}"));
        std::fs::create_dir_all(&dir).expect("create tempdir");
        dir
    }
}
