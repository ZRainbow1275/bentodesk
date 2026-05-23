//! T-094a — Hide-side operations of the stealth subsystem.
//!
//! Owns the *putting-files-into-`.bentodesk/`* half of the original 1.x
//! `hidden_items.rs`:
//!
//! - [`hidden_dir_for`] / [`zone_hidden_dir_for`] — resolve and create the
//!   hidden tree on demand, stamping `HIDDEN | SYSTEM | NOT_CONTENT_INDEXED`
//!   on every fresh subdirectory via Win32 `SetFileAttributesW`.
//! - [`apply_stealth_attrs`] — direct Win32 attribute mutation. Replaces
//!   1.x's child-process `attrib +h +s` spawn (eliminates `CREATE_NO_WINDOW`
//!   races and Unicode truncation, surfaces `GetLastError` for the retry
//!   queue).
//! - [`ensure_stealth`] — idempotent re-apply, fast-path when the bits are
//!   already set, queues retry on lock contention.
//! - [`hide_file`] — physically move a file from the desktop into
//!   `.bentodesk/{zone_id}/`, recording it in the safety manifest via
//!   [`crate::stealth::sync::manifest_add`].
//!
//! The companion modules [`crate::stealth::restore`] handles the inverse
//! motion (`.bentodesk/` → desktop), and [`crate::stealth::sync`] handles
//! periodic sweeping + manifest persistence + legacy migration.

use std::path::{Path, PathBuf};

use crossbeam_channel::Sender;

use super::{
    StealthConfig, StealthError, StealthEvent, paths_equal_str, record_failure,
    record_retry_drained, record_success, status, unique_suffix,
};

#[cfg(windows)]
use super::{INVALID_FILE_ATTRIBUTES, STEALTH_ATTRS};

// ─── Hidden directory resolution ─────────────────────────────────────────

/// Resolve `{desktop_path}/.bentodesk/`. Creates the directory and stamps
/// stealth attributes on first call. Refuses to resolve against an empty or
/// relative `desktop_path` (returning `Err`) — historically that race
/// condition wrote `manifest.json` into the install dir and then quarantined
/// it onto the actual desktop on next launch.
///
/// 1.x signature was `hidden_dir(handle: &AppHandle) -> PathBuf` and read
/// `desktop_path` from `AppState.settings`. The nano port takes
/// [`StealthConfig`] explicitly so the caller owns settings ingestion.
pub fn hidden_dir_for(config: &StealthConfig) -> Result<PathBuf, StealthError> {
    let trimmed = config.desktop_path.trim();
    if trimmed.is_empty() {
        tracing::error!("hidden_dir_for: desktop_path is empty");
        return Err(StealthError::InvalidDesktopPath {
            value: trimmed.to_string(),
        });
    }
    let abs = PathBuf::from(trimmed);
    if !abs.is_absolute() {
        tracing::error!("hidden_dir_for: desktop_path is not absolute: {trimmed:?}");
        return Err(StealthError::InvalidDesktopPath {
            value: trimmed.to_string(),
        });
    }

    let dir = abs.join(".bentodesk");

    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| StealthError::Io {
            path: dir.clone(),
            message: e.to_string(),
        })?;
        tracing::info!("Created .bentodesk/ directory: {:?}", dir);
    }

    ensure_stealth(&dir);

    Ok(dir)
}

/// Resolve `{desktop_path}/.bentodesk/{zone_id}/`. Creates the directory
/// (and the parent hidden tree) on demand. Stamps stealth attributes on the
/// zone subdir even though Explorer would inherit from the parent — Windows
/// Search and third-party indexers do *not* honour the chain.
pub fn zone_hidden_dir_for(config: &StealthConfig, zone_id: &str) -> Result<PathBuf, StealthError> {
    let parent = hidden_dir_for(config)?;
    let dir = parent.join(zone_id);

    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| StealthError::Io {
            path: dir.clone(),
            message: e.to_string(),
        })?;
        tracing::info!("Created zone hidden dir: {:?}", dir);
    }

    ensure_stealth(&dir);

    Ok(dir)
}

/// Remove an empty zone subdirectory from `.bentodesk/`.
///
/// Refuses to remove if any user file remains (returns `Ok(false)`). The
/// caller is expected to invoke `restore_zone_items_with_dirs` first.
pub fn cleanup_zone_dir(config: &StealthConfig, zone_id: &str) -> Result<bool, StealthError> {
    let parent = hidden_dir_for(config)?;
    let dir = parent.join(zone_id);

    if !dir.exists() {
        return Ok(true);
    }

    match std::fs::read_dir(&dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                tracing::warn!(
                    "cleanup_zone_dir: zone dir {:?} not empty, refusing to remove",
                    dir
                );
                return Ok(false);
            }
        }
        Err(e) => {
            return Err(StealthError::Io {
                path: dir.clone(),
                message: e.to_string(),
            });
        }
    }

    match std::fs::remove_dir(&dir) {
        Ok(()) => {
            tracing::info!("Removed empty zone dir: {:?}", dir);
            Ok(true)
        }
        Err(e) => {
            tracing::warn!("Failed to remove zone dir {:?}: {}", dir, e);
            Err(StealthError::Io {
                path: dir,
                message: e.to_string(),
            })
        }
    }
}

// ─── Win32 attribute application ─────────────────────────────────────────

/// Encode a path as a null-terminated UTF-16 buffer for Win32 wide-string APIs.
#[cfg(windows)]
fn to_wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Read the Win32 attribute mask for `path`. Returns `None` when the call
/// fails (path missing, permission denied, ...) — callers treat that as
/// "stamp it anyway".
///
/// Uses `windows-sys` (plain Win32, no COM) per spec §3.1.1 — `windows`
/// 0.58 is reserved for COM-typed graphics interfaces.
#[cfg(windows)]
fn read_current_attrs(path: &Path) -> Option<u32> {
    use windows_sys::Win32::Storage::FileSystem::GetFileAttributesW;

    let wide = to_wide_path(path);
    // SAFETY: `wide` is a valid null-terminated UTF-16 buffer that lives
    // for the duration of the call. `GetFileAttributesW` has no further
    // preconditions and never writes through the pointer.
    let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        None
    } else {
        Some(attrs)
    }
}

/// Apply the stealth attribute bundle to `path`, OR-ing the bits into the
/// existing mask so pre-existing flags (e.g. `FILE_ATTRIBUTE_DIRECTORY`)
/// are preserved.
///
/// Returns `Err(String)` carrying a human-readable Win32 error code on
/// failure so the caller can surface it via `StealthEvent::StatusChanged`.
#[cfg(windows)]
pub fn apply_stealth_attrs(path: &Path) -> Result<(), String> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Storage::FileSystem::SetFileAttributesW;

    let wide = to_wide_path(path);
    let existing = read_current_attrs(path).unwrap_or(0);
    let new_attrs = existing | STEALTH_ATTRS;

    if existing != 0 && (existing & STEALTH_ATTRS) == STEALTH_ATTRS {
        return Ok(());
    }

    // SAFETY: `wide` is a valid null-terminated UTF-16 buffer alive for the
    // call. The flag mask is a plain `u32` consumed by value. `windows-sys`
    // returns `BOOL` (0 = failure); GetLastError is read immediately on
    // failure with no intervening Win32 calls.
    let ok = unsafe { SetFileAttributesW(wide.as_ptr(), new_attrs) };
    if ok != 0 {
        Ok(())
    } else {
        // SAFETY: GetLastError has no preconditions and reads thread-local TLS.
        let code = unsafe { GetLastError() };
        Err(format!("SetFileAttributesW failed (GetLastError={code})"))
    }
}

/// Non-Windows stub for cross-compile cleanliness. Filesystem has no
/// equivalent of the Win32 hidden/system bundle; the `.bentodesk/`
/// dot-prefix already suffices for Unix `ls` / Finder visibility.
#[cfg(not(windows))]
pub fn apply_stealth_attrs(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Idempotent re-apply of the stealth attribute bundle on `path`.
///
/// No-op when the attributes are already present. Records failure into the
/// retry queue (drained by [`crate::stealth::sync::AttrGuard::sweep`]) and
/// success into the live status snapshot.
pub fn ensure_stealth(path: &Path) {
    #[cfg(windows)]
    {
        if !path.exists() {
            return;
        }
        match apply_stealth_attrs(path) {
            Ok(()) => {
                record_success();
                record_retry_drained(path);
            }
            Err(e) => {
                tracing::warn!(
                    "ensure_stealth: could not apply to {}: {}",
                    path.display(),
                    e
                );
                record_failure(&e, path);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
    }
}

// ─── Filename uniqueness ────────────────────────────────────────────────

/// Build a destination path inside `hidden_dir`, appending an 8-char hex
/// suffix to disambiguate when a file with the same name already exists.
///
/// The 1.x version used `uuid::Uuid::new_v4()`; spec §8 doesn't whitelist
/// `uuid` for this crate so the nano port substitutes a SystemTime + atomic
/// counter blend (see [`crate::stealth::unique_suffix`]). Equivalent
/// uniqueness for the per-zone-subdir scope.
pub fn unique_hidden_path(hidden_dir: &Path, original: &Path) -> PathBuf {
    let file_name = original
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let candidate = hidden_dir.join(&file_name);
    if !candidate.exists() {
        return candidate;
    }

    let stem = original
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let ext = original
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let suffix = unique_suffix();

    hidden_dir.join(format!("{stem}_{suffix}{ext}"))
}

// ─── Hide entry point ───────────────────────────────────────────────────

/// Hide `file_path` by moving it into `.bentodesk/{zone_id}/`.
///
/// Returns `(original_path, hidden_path)` on success. On any failure the
/// original file stays at its desktop location (safe default).
///
/// `events` is an optional broadcast channel — when `Some`, a
/// [`StealthEvent::Hidden`] payload is pushed on success and a
/// [`StealthEvent::StatusChanged`] is pushed on every status mutation. Pass
/// `None` from tests / one-shot callers that don't need the event stream.
///
/// 1.x signature was `hide_file(handle: &AppHandle, file_path, zone_id)`.
/// The nano port takes [`StealthConfig`] explicitly + hands manifest
/// persistence to [`crate::stealth::sync::manifest_add`].
pub fn hide_file(
    config: &StealthConfig,
    file_path: &str,
    zone_id: &str,
    file_type: &str,
    icon_x: Option<i32>,
    icon_y: Option<i32>,
    events: Option<&Sender<StealthEvent>>,
) -> Result<(String, String), StealthError> {
    let source = Path::new(file_path);
    if !source.exists() {
        tracing::warn!("Cannot hide file — does not exist: {}", file_path);
        return Err(StealthError::HideSourceMissing {
            path: source.to_path_buf(),
        });
    }

    let hdir = zone_hidden_dir_for(config, zone_id)?;
    let dest = unique_hidden_path(&hdir, source);

    // Same-drive rename should be instant.
    if let Err(e) = std::fs::rename(source, &dest) {
        tracing::warn!(
            "fs::rename failed for hide ({} -> {:?}): {}. Trying copy+delete fallback.",
            file_path,
            dest,
            e
        );
        // Cross-drive fallback. Rare since `.bentodesk/` lives on the
        // desktop drive but possible if the user mounts Desktop on a
        // junction point that crosses volumes.
        match std::fs::copy(source, &dest) {
            Ok(_) => {
                if let Err(e2) = std::fs::remove_file(source) {
                    tracing::error!(
                        "Copy succeeded but delete of original failed: {}. Removing copy to stay safe.",
                        e2
                    );
                    let _ = std::fs::remove_file(&dest);
                    return Err(StealthError::Io {
                        path: source.to_path_buf(),
                        message: e2.to_string(),
                    });
                }
            }
            Err(e2) => {
                tracing::error!(
                    "Copy fallback also failed ({} -> {:?}): {}",
                    file_path,
                    dest,
                    e2
                );
                return Err(StealthError::Io {
                    path: source.to_path_buf(),
                    message: e2.to_string(),
                });
            }
        }
    }

    let file_size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);

    // Re-assert stealth on the zone dir — Explorer caches attributes and a
    // fresh write may briefly expose the dir as non-superhidden.
    if let Some(zone_dir) = dest.parent() {
        ensure_stealth(zone_dir);
    }

    let hidden_path_str = dest.to_string_lossy().into_owned();
    let display_name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Track in the global manifest. Failures here are logged but do NOT
    // unwind the move — the file is already physically hidden, the
    // manifest update is recovery metadata.
    let global_dir = hidden_dir_for(config)?;
    if let Err(e) = super::sync::manifest_add(
        &global_dir,
        super::sync::ManifestAddParams {
            original_path: file_path,
            hidden_path: &hidden_path_str,
            zone_id,
            file_size_bytes: file_size,
            display_name: &display_name,
            icon_x,
            icon_y,
            file_type,
        },
    ) {
        tracing::warn!(
            "hide_file: manifest_add failed for {file_path}: {e} — file is hidden but unrecorded"
        );
    }

    tracing::info!(
        "Hidden desktop item (zone '{}'): {} -> {}",
        zone_id,
        file_path,
        hidden_path_str
    );

    if let Some(tx) = events {
        let _ = tx.send(StealthEvent::Hidden {
            original: file_path.to_string(),
            hidden: hidden_path_str.clone(),
        });
        let _ = tx.send(StealthEvent::StatusChanged(status()));
    }

    Ok((file_path.to_string(), hidden_path_str))
}

/// Per-file outcome for [`hide_zone`] / [`hide_pattern`]: input path paired
/// with either the resulting hidden path (success) or the failure reason.
pub type HideOutcome = (String, Result<String, StealthError>);

/// Hide every file under `zone_id` in one batch. Returns a per-file outcome
/// map so the caller can render partial-failure UI without losing the
/// per-item reason.
pub fn hide_zone(
    config: &StealthConfig,
    zone_id: &str,
    file_paths: &[&str],
    events: Option<&Sender<StealthEvent>>,
) -> Vec<HideOutcome> {
    file_paths
        .iter()
        .map(|fp| {
            let result =
                hide_file(config, fp, zone_id, "", None, None, events).map(|(_, hidden)| hidden);
            (fp.to_string(), result)
        })
        .collect()
}

/// Hide every desktop file whose name matches `pattern` (case-insensitive
/// substring match). Used by the "stealth pattern" rule type — see 1.x
/// `commands/stealth.rs::hide_by_pattern`.
///
/// Returns the per-file outcome map (same shape as [`hide_zone`]).
pub fn hide_pattern(
    config: &StealthConfig,
    zone_id: &str,
    pattern: &str,
    events: Option<&Sender<StealthEvent>>,
) -> Result<Vec<HideOutcome>, StealthError> {
    let desktop = PathBuf::from(config.desktop_path.as_str());
    if !desktop.is_dir() {
        return Err(StealthError::InvalidDesktopPath {
            value: config.desktop_path.to_string(),
        });
    }

    let pattern_lc = pattern.to_lowercase();
    let mut targets: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(&desktop).map_err(|e| StealthError::Io {
        path: desktop.clone(),
        message: e.to_string(),
    })? {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("hide_pattern: read_dir entry error: {e}");
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.contains(&pattern_lc) {
            targets.push(path.to_string_lossy().into_owned());
        }
    }

    let refs: Vec<&str> = targets.iter().map(String::as_str).collect();
    Ok(hide_zone(config, zone_id, &refs, events))
}

// ─── Path predicate (re-export of paths_equal_str friendliness) ──────────

/// Convenience: `true` when `a` and `b` refer to the same file (case +
/// separator insensitive), without requiring either path to exist.
pub fn paths_equal(a: &str, b: &str) -> bool {
    paths_equal_str(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_for(desktop: &Path) -> StealthConfig {
        StealthConfig {
            desktop_path: smol_str::SmolStr::new(desktop.to_string_lossy()),
            app_data_dir: smol_str::SmolStr::new(desktop.to_string_lossy()),
        }
    }

    #[test]
    fn hidden_dir_rejects_empty_desktop_path() {
        let cfg = StealthConfig {
            desktop_path: smol_str::SmolStr::new_static(""),
            app_data_dir: smol_str::SmolStr::new_static(""),
        };
        let result = hidden_dir_for(&cfg);
        assert!(matches!(
            result,
            Err(StealthError::InvalidDesktopPath { .. })
        ));
    }

    #[test]
    fn hidden_dir_rejects_relative_desktop_path() {
        let cfg = StealthConfig {
            desktop_path: smol_str::SmolStr::new_static("relative/path"),
            app_data_dir: smol_str::SmolStr::new_static(""),
        };
        let result = hidden_dir_for(&cfg);
        assert!(matches!(
            result,
            Err(StealthError::InvalidDesktopPath { .. })
        ));
    }

    #[test]
    fn hidden_dir_creates_dotdir() {
        let tmp = tempdir();
        let cfg = config_for(tmp.as_path());
        let dir = hidden_dir_for(&cfg).expect("hidden_dir");
        assert!(dir.is_dir());
        assert_eq!(dir.file_name().and_then(|s| s.to_str()), Some(".bentodesk"));
    }

    #[test]
    fn zone_hidden_dir_creates_subdir() {
        let tmp = tempdir();
        let cfg = config_for(tmp.as_path());
        let dir = zone_hidden_dir_for(&cfg, "zone-x").expect("zone_dir");
        assert!(dir.is_dir());
        assert_eq!(dir.file_name().and_then(|s| s.to_str()), Some("zone-x"));
    }

    #[test]
    fn unique_hidden_path_no_collision_returns_plain_name() {
        let tmp = tempdir();
        let dest = unique_hidden_path(tmp.as_path(), Path::new("foo.txt"));
        assert_eq!(dest.file_name().and_then(|s| s.to_str()), Some("foo.txt"));
    }

    #[test]
    fn unique_hidden_path_collision_appends_suffix() {
        let tmp = tempdir();
        let existing = tmp.as_path().join("foo.txt");
        std::fs::write(&existing, b"x").expect("write");
        let dest = unique_hidden_path(tmp.as_path(), Path::new("foo.txt"));
        let name = dest
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        assert!(name.starts_with("foo_"));
        assert!(name.ends_with(".txt"));
        assert_ne!(name, "foo.txt");
    }

    #[test]
    fn hide_file_moves_source_into_zone_subdir() {
        let tmp = tempdir();
        let cfg = config_for(tmp.as_path());

        let src = tmp.as_path().join("doc.txt");
        std::fs::write(&src, b"hello").expect("seed");

        let (orig, hidden) = hide_file(
            &cfg,
            &src.to_string_lossy(),
            "zone-1",
            "File",
            None,
            None,
            None,
        )
        .expect("hide_file");

        assert_eq!(orig, src.to_string_lossy());
        assert!(!src.exists(), "source should have been moved");
        assert!(Path::new(&hidden).exists(), "destination should exist");
        assert!(hidden.contains(".bentodesk"));
        assert!(hidden.contains("zone-1"));
    }

    #[test]
    fn hide_file_returns_err_when_source_missing() {
        let tmp = tempdir();
        let cfg = config_for(tmp.as_path());
        let nonexistent = tmp.as_path().join("nope.txt");
        let result = hide_file(
            &cfg,
            &nonexistent.to_string_lossy(),
            "zone-1",
            "File",
            None,
            None,
            None,
        );
        assert!(matches!(
            result,
            Err(StealthError::HideSourceMissing { .. })
        ));
    }

    #[test]
    fn cleanup_zone_dir_refuses_when_files_remain() {
        let tmp = tempdir();
        let cfg = config_for(tmp.as_path());
        let zone_dir = zone_hidden_dir_for(&cfg, "z").expect("zone dir");
        std::fs::write(zone_dir.join("a.txt"), b"x").expect("seed");
        let removed = cleanup_zone_dir(&cfg, "z").expect("cleanup");
        assert!(!removed, "should refuse to remove non-empty dir");
        assert!(zone_dir.exists());
    }

    #[test]
    fn cleanup_zone_dir_removes_empty_dir() {
        let tmp = tempdir();
        let cfg = config_for(tmp.as_path());
        let zone_dir = zone_hidden_dir_for(&cfg, "z").expect("zone dir");
        let removed = cleanup_zone_dir(&cfg, "z").expect("cleanup");
        assert!(removed);
        assert!(!zone_dir.exists());
    }

    // ── tempdir helper (no `tempfile` workspace dep — we hand-roll) ───

    struct TmpDir(PathBuf);

    impl TmpDir {
        fn as_path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tempdir() -> TmpDir {
        let base = std::env::temp_dir();
        let suffix = unique_suffix();
        let path = base.join(format!("bento-stealth-{}-{}", std::process::id(), suffix));
        std::fs::create_dir_all(&path).expect("tempdir");
        TmpDir(path)
    }
}
