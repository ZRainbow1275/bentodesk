//! Live Folder Sync — bind a filesystem folder to a [`ZoneId`] (port of 1.x
//! `watcher::live_folder`).
//!
//! Subscribes the OS to a user-chosen folder; when *any* file under the folder
//! changes, the corresponding zone receives a [`ZoneRefreshEvent`] so its UI
//! can re-scan and re-render.
//!
//! ## Differences vs 1.x
//!
//! - Zone IDs are typed [`ZoneId`] (u64) rather than strings — matches the
//!   nano `bento-nano-zone` domain model.
//! - Output goes via a caller-supplied `crossbeam_channel::Sender`. The
//!   1.x module emitted a Tauri event (`zone_live_refresh`).
//! - State storage is a process-local singleton wrapped in the same
//!   `LazyLock<RwLock<...>>` pattern 1.x used — calling `bind` / `unbind`
//!   from multiple threads is safe.
//! - `rehydrate_from_layout` is dropped (Tauri-runtime-coupled). Callers
//!   walk their persisted layout and call [`bind`] for each entry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock, RwLock};
use std::time::Duration;

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

use bento_nano_zone::ZoneId;

use super::debouncer::{DebouncedEvent, Debouncer, DebouncerError};

/// Folders we refuse to bind — binding them could cause mass data churn or
/// leak system-owned paths into the UI. List preserved verbatim from 1.x.
pub const BLACKLISTED_PREFIXES: &[&str] = &[
    r"C:\Windows",
    r"C:\Program Files",
    r"C:\Program Files (x86)",
    r"C:\ProgramData",
    r"C:\System Volume Information",
    r"C:\$Recycle.Bin",
];

/// Errors surfaced by the live-folder API.
#[derive(Debug)]
pub enum LiveFolderError {
    EmptyPath,
    NotFound(PathBuf),
    NotADirectory(PathBuf),
    Blacklisted(PathBuf),
    DriveRoot(PathBuf),
    NotInitialised,
    Debouncer(DebouncerError),
    AlreadyInitialised,
}

impl core::fmt::Display for LiveFolderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyPath => f.write_str("empty folder path"),
            Self::NotFound(p) => write!(f, "folder does not exist: {}", p.display()),
            Self::NotADirectory(p) => write!(f, "not a directory: {}", p.display()),
            Self::Blacklisted(p) => {
                write!(f, "refusing to bind system folder: {}", p.display())
            }
            Self::DriveRoot(p) => write!(f, "refusing to bind a root drive: {}", p.display()),
            Self::NotInitialised => f.write_str("live folder watcher not initialised"),
            Self::Debouncer(e) => write!(f, "live folder debouncer: {e}"),
            Self::AlreadyInitialised => f.write_str("live folder watcher already initialised"),
        }
    }
}

impl core::error::Error for LiveFolderError {}

impl From<DebouncerError> for LiveFolderError {
    fn from(value: DebouncerError) -> Self {
        Self::Debouncer(value)
    }
}

/// Reject folders whose path doesn't look like a sensible user dir.
///
/// Mirrors 1.x rejection rules: empty path, non-existent, non-directory,
/// blacklisted system folder, or drive root (`C:\`, `D:\`).
pub fn validate_folder(path: &Path) -> Result<(), LiveFolderError> {
    let p = path.to_string_lossy().to_string();
    if p.is_empty() {
        return Err(LiveFolderError::EmptyPath);
    }
    if !path.exists() {
        return Err(LiveFolderError::NotFound(path.to_path_buf()));
    }
    if !path.is_dir() {
        return Err(LiveFolderError::NotADirectory(path.to_path_buf()));
    }

    let lower = p.to_lowercase();
    for prefix in BLACKLISTED_PREFIXES {
        if lower.starts_with(&prefix.to_lowercase()) {
            return Err(LiveFolderError::Blacklisted(path.to_path_buf()));
        }
    }

    // Refuse plain drive roots — almost always the wrong scope.
    let components = path.components().count();
    if components <= 2 {
        return Err(LiveFolderError::DriveRoot(path.to_path_buf()));
    }

    Ok(())
}

/// Payload pushed when any file under a bound folder changes.
///
/// Carries only the [`ZoneId`] — consumers re-scan the bound folder
/// themselves rather than try to reconcile per-file events. Matches 1.x
/// behaviour where `zone_live_refresh` carried only the zone id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneRefreshEvent {
    pub zone_id: ZoneId,
}

// ─── Process-local state ────────────────────────────────────────────

struct LiveFolderState {
    debouncer: Debouncer,
}

static BINDINGS: LazyLock<RwLock<HashMap<PathBuf, ZoneId>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
static STATE: Mutex<Option<LiveFolderState>> = Mutex::new(None);
/// The caller's emit channel. Set once at first `ensure_initialised`; a
/// second initialisation with a different channel returns `AlreadyInitialised`
/// to surface the misuse explicitly.
static EMITTER: OnceLock<Sender<ZoneRefreshEvent>> = OnceLock::new();

/// Initialise the shared live-folder debouncer. Idempotent: calling twice
/// with the same `out` is a no-op; calling twice with a different sender
/// returns `AlreadyInitialised`.
pub fn ensure_initialised(out: Sender<ZoneRefreshEvent>) -> Result<(), LiveFolderError> {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_some() {
        if EMITTER.get().is_some_and(|s| !s.same_channel(&out)) {
            return Err(LiveFolderError::AlreadyInitialised);
        }
        return Ok(());
    }

    if EMITTER.set(out).is_err() {
        // Race: lost the set; tolerate (Sender already present).
    }

    let debouncer = Debouncer::start(Duration::from_millis(300), |batch| {
        dispatch_events(batch);
    })?;

    *guard = Some(LiveFolderState { debouncer });
    Ok(())
}

fn dispatch_events(events: Vec<DebouncedEvent>) {
    let bindings = match BINDINGS.read() {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("live_folder: bindings poisoned: {e}");
            return;
        }
    };

    let mut zones_to_refresh: smallvec::SmallVec<[ZoneId; 8]> = smallvec::SmallVec::new();

    for ev in events {
        for p in &ev.paths {
            for (folder, zone_id) in bindings.iter() {
                if p.starts_with(folder) {
                    if !zones_to_refresh.contains(zone_id) {
                        zones_to_refresh.push(*zone_id);
                    }
                    break;
                }
            }
        }
    }

    let Some(sender) = EMITTER.get() else {
        return;
    };
    for zone_id in zones_to_refresh {
        if sender.send(ZoneRefreshEvent { zone_id }).is_err() {
            // Receiver dropped — silently abort further sends this batch.
            return;
        }
    }
}

/// Bind a zone to a folder. Subsequent events under the folder push a
/// [`ZoneRefreshEvent`] carrying the zone id.
///
/// Requires [`ensure_initialised`] to have been called previously; returns
/// `NotInitialised` otherwise. Validation rejects empty / non-existent /
/// blacklisted / drive-root paths.
pub fn bind(zone_id: ZoneId, folder: &Path) -> Result<(), LiveFolderError> {
    validate_folder(folder)?;

    let guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = guard.as_ref().ok_or(LiveFolderError::NotInitialised)?;
    state.debouncer.watch(folder, false)?;

    let mut bindings = BINDINGS.write().unwrap_or_else(|e| e.into_inner());
    bindings.insert(folder.to_path_buf(), zone_id);

    tracing::info!(
        "live_folder: bound zone {:?} -> {}",
        zone_id,
        folder.display()
    );
    Ok(())
}

/// Release the zone's folder binding. No-op when the zone isn't bound.
///
/// Best-effort: also pushes one `ZoneRefreshEvent` so the consumer clears
/// any cached live-view state.
pub fn unbind(zone_id: ZoneId) -> Result<(), LiveFolderError> {
    let guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return Ok(()),
    };

    let mut bindings = BINDINGS.write().unwrap_or_else(|e| e.into_inner());
    let mut removed: smallvec::SmallVec<[PathBuf; 4]> = smallvec::SmallVec::new();
    bindings.retain(|folder, zid| {
        if *zid == zone_id {
            removed.push(folder.clone());
            false
        } else {
            true
        }
    });
    drop(bindings);

    for folder in removed {
        let _ = state.debouncer.unwatch(&folder);
    }

    if let Some(sender) = EMITTER.get() {
        let _ = sender.send(ZoneRefreshEvent { zone_id });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_folder() {
        let p = PathBuf::from(r"Z:\definitely\not\a\real\folder");
        assert!(matches!(
            validate_folder(&p),
            Err(LiveFolderError::NotFound(_))
        ));
    }

    #[test]
    fn rejects_windows_dir() {
        let p = PathBuf::from(r"C:\Windows");
        // Windows directory exists on a Windows host; on other hosts the
        // NotFound branch fires first. Both are acceptable rejections.
        let res = validate_folder(&p);
        assert!(matches!(
            res,
            Err(LiveFolderError::Blacklisted(_)) | Err(LiveFolderError::NotFound(_))
        ));
    }

    #[test]
    fn rejects_drive_root() {
        let p = PathBuf::from(r"C:\");
        let res = validate_folder(&p);
        assert!(matches!(
            res,
            Err(LiveFolderError::DriveRoot(_)) | Err(LiveFolderError::NotFound(_))
        ));
    }

    #[test]
    fn zone_refresh_event_serde_round_trip() {
        let original = ZoneRefreshEvent { zone_id: ZoneId(7) };
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: ZoneRefreshEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, parsed);
    }
}
