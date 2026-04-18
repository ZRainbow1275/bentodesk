//! Live Folder Sync — Mirror an arbitrary filesystem folder into a Zone (read-only).
//!
//! The zone's items list is rebuilt on each fs event so the view always
//! reflects the underlying folder. **No file content changes propagate the
//! other way** in this MVP (zone edits do NOT rename/move real files).
//!
//! A standalone `Debouncer` is used so that live-folder events never mix
//! with the main desktop watcher stream.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, RwLock};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebouncedEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::error::BentoDeskError;

/// Folders we refuse to bind — binding them could cause mass data churn or
/// leak system-owned paths into the UI.
pub const BLACKLISTED_PREFIXES: &[&str] = &[
    r"C:\Windows",
    r"C:\Program Files",
    r"C:\Program Files (x86)",
    r"C:\ProgramData",
    r"C:\System Volume Information",
    r"C:\$Recycle.Bin",
];

/// Reject folders whose path doesn't look like a sensible user dir.
pub fn validate_folder(path: &Path) -> Result<(), BentoDeskError> {
    let p = path.to_string_lossy().to_string();
    if p.is_empty() {
        return Err(BentoDeskError::Generic("Empty folder path".into()));
    }
    if !path.exists() {
        return Err(BentoDeskError::Generic(format!(
            "Folder does not exist: {p}"
        )));
    }
    if !path.is_dir() {
        return Err(BentoDeskError::Generic(format!("Not a directory: {p}")));
    }

    let lower = p.to_lowercase();
    for prefix in BLACKLISTED_PREFIXES {
        if lower.starts_with(&prefix.to_lowercase()) {
            return Err(BentoDeskError::Generic(format!(
                "Refusing to bind system folder: {p}"
            )));
        }
    }

    // Refuse plain drive roots (C:\, D:\) — almost always the wrong scope.
    let components = path.components().count();
    if components <= 2 {
        return Err(BentoDeskError::Generic(format!(
            "Refusing to bind a root drive: {p}"
        )));
    }

    Ok(())
}

type LiveDebouncer =
    notify_debouncer_full::Debouncer<RecommendedWatcher, notify_debouncer_full::FileIdMap>;

struct LiveFolderState {
    debouncer: Option<LiveDebouncer>,
    /// folder → zone_id map. Mutated under its own RwLock so debouncer
    /// callback can read without blocking binds.
    bindings: &'static RwLock<HashMap<PathBuf, String>>,
}

static BINDINGS: LazyLock<RwLock<HashMap<PathBuf, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
static STATE: Mutex<Option<LiveFolderState>> = Mutex::new(None);

/// Initialise the shared live-folder debouncer. Idempotent: calling twice is
/// a no-op, which makes it safe to invoke from multiple `bind` callers.
pub fn ensure_initialised(handle: &AppHandle) -> Result<(), BentoDeskError> {
    let mut guard = STATE
        .lock()
        .map_err(|e| BentoDeskError::Generic(format!("live folder lock: {e}")))?;
    if guard.is_some() {
        return Ok(());
    }

    let handle_clone = handle.clone();

    let debouncer = new_debouncer(
        Duration::from_millis(300),
        None,
        move |events: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| match events {
            Ok(evs) => dispatch_events(&handle_clone, evs),
            Err(errs) => {
                for e in errs {
                    tracing::warn!("live_folder watcher error: {e}");
                }
            }
        },
    )?;

    *guard = Some(LiveFolderState {
        debouncer: Some(debouncer),
        bindings: &BINDINGS,
    });
    Ok(())
}

fn dispatch_events(handle: &AppHandle, events: Vec<DebouncedEvent>) {
    let bindings = match BINDINGS.read() {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("live_folder bindings poisoned: {e}");
            return;
        }
    };

    // Coalesce into one event per zone: any matching path triggers a refresh.
    let mut zones_to_refresh: std::collections::HashSet<String> = std::collections::HashSet::new();

    for ev in events {
        for p in &ev.event.paths {
            for (folder, zone_id) in bindings.iter() {
                if p.starts_with(folder) {
                    zones_to_refresh.insert(zone_id.clone());
                    break;
                }
            }
        }
    }

    for zone_id in zones_to_refresh {
        if let Err(e) = handle.emit("zone_live_refresh", &zone_id) {
            tracing::warn!("Failed to emit zone_live_refresh: {e}");
        }
    }
}

/// Bind a zone to a folder. Subsequent events under the folder emit
/// `zone_live_refresh` carrying the zone id as payload.
pub fn bind(handle: &AppHandle, zone_id: &str, folder: &Path) -> Result<(), BentoDeskError> {
    validate_folder(folder)?;
    ensure_initialised(handle)?;

    let mut guard = STATE
        .lock()
        .map_err(|e| BentoDeskError::Generic(format!("live folder lock: {e}")))?;
    let state = guard
        .as_mut()
        .ok_or_else(|| BentoDeskError::Generic("live folder state missing".into()))?;
    let debouncer = state
        .debouncer
        .as_mut()
        .ok_or_else(|| BentoDeskError::Generic("live folder debouncer missing".into()))?;

    debouncer.watch(folder, RecursiveMode::NonRecursive)?;

    let mut bindings = state
        .bindings
        .write()
        .map_err(|e| BentoDeskError::Generic(format!("live folder bindings write: {e}")))?;
    bindings.insert(folder.to_path_buf(), zone_id.to_string());

    tracing::info!("live_folder: bound zone {} → {}", zone_id, folder.display());
    Ok(())
}

/// Release the zone's folder binding. No-op when the zone isn't bound.
pub fn unbind(handle: &AppHandle, zone_id: &str) -> Result<(), BentoDeskError> {
    let mut guard = STATE
        .lock()
        .map_err(|e| BentoDeskError::Generic(format!("live folder lock: {e}")))?;
    let state = match guard.as_mut() {
        Some(s) => s,
        None => return Ok(()),
    };
    let debouncer = match state.debouncer.as_mut() {
        Some(d) => d,
        None => return Ok(()),
    };
    let mut bindings = state
        .bindings
        .write()
        .map_err(|e| BentoDeskError::Generic(format!("live folder bindings write: {e}")))?;
    let mut removed = Vec::new();
    bindings.retain(|folder, zid| {
        if zid == zone_id {
            removed.push(folder.clone());
            false
        } else {
            true
        }
    });
    for folder in removed {
        let _ = debouncer.unwatch(&folder);
    }

    // Best-effort: poke the frontend so it clears the live view.
    let _ = handle.emit("zone_live_refresh", zone_id);
    Ok(())
}

/// Rebind bindings from persisted state at startup. Called from lib.rs setup.
pub fn rehydrate_from_layout(handle: &AppHandle) {
    let state = match handle.try_state::<crate::AppState>() {
        Some(s) => s,
        None => return,
    };
    let layout = match state.layout.lock() {
        Ok(l) => l.clone(),
        Err(e) => {
            tracing::warn!("live_folder rehydrate: layout lock poisoned: {e}");
            return;
        }
    };
    for zone in &layout.zones {
        if let Some(path) = &zone.live_folder_path {
            let pb = PathBuf::from(path);
            if let Err(e) = bind(handle, &zone.id, &pb) {
                tracing::warn!(
                    "live_folder: rehydrate bind failed for zone {} → {}: {e}",
                    zone.id,
                    path
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_folder() {
        let p = PathBuf::from(r"Z:\definitely\not\a\real\folder");
        assert!(validate_folder(&p).is_err());
    }

    #[test]
    fn rejects_windows_dir() {
        let p = PathBuf::from(r"C:\Windows");
        assert!(validate_folder(&p).is_err());
    }

    #[test]
    fn rejects_drive_root() {
        let p = PathBuf::from(r"C:\");
        assert!(validate_folder(&p).is_err());
    }
}
