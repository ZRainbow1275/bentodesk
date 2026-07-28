//! Desktop directory watcher (port of 1.x `watcher::desktop_watcher`).
//!
//! Watches one or more *Desktop* directories — the User Desktop, the Public
//! Desktop, an optional OneDrive Desktop redirect, and an optional user-chosen
//! override — and forwards [`FileChangedEvent`]s through a caller-supplied
//! `crossbeam_channel::Sender`.
//!
//! The 1.x version pulled the source list from
//! `crate::desktop_sources::all_desktop_dirs(custom_ref)` (out of T-082 scope —
//! that lives in T-093). Here, the caller passes the resolved paths in. Failing
//! to attach to *every* path is a hard error (matches 1.x behaviour: silent
//! degradation to a no-op watcher leaves the user with no diagnostic).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use super::debouncer::{DebouncedEvent, Debouncer, DebouncerError};

/// Errors surfaced by [`setup_file_watcher`].
#[derive(Debug)]
pub enum WatcherError {
    /// The caller supplied no source paths.
    NoSources,
    /// Failed to build the underlying debouncer / `notify` watcher.
    Debouncer(DebouncerError),
    /// None of the supplied source paths could be attached.
    NoSourcesAttached { attempted: usize },
}

impl core::fmt::Display for WatcherError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoSources => f.write_str("no desktop source paths supplied"),
            Self::Debouncer(e) => write!(f, "debouncer init failed: {e}"),
            Self::NoSourcesAttached { attempted } => write!(
                f,
                "failed to attach watcher to any of {attempted} desktop source(s)"
            ),
        }
    }
}

impl core::error::Error for WatcherError {}

impl From<DebouncerError> for WatcherError {
    fn from(value: DebouncerError) -> Self {
        Self::Debouncer(value)
    }
}

/// Kind of change that happened to a file. Matches the 1.x payload's
/// `event_type` string but carried as a typed enum so consumers can `match`.
///
/// `Serialize` / `Deserialize` are present per the master plan §11 ΔB ruling
/// (forward-compat for v2.x scripting hooks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Create,
    Modify,
    Delete,
}

impl ChangeKind {
    /// String name matching the 1.x `event_type` field on `FileChangedPayload`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Modify => "modify",
            Self::Delete => "delete",
        }
    }
}

/// Payload emitted when a file in a watched directory changes.
///
/// Field shape mirrors the 1.x `FileChangedPayload` byte-for-byte so that
/// future v2.x scripting / IPC re-introduction can serialize it without
/// schema churn (master plan §11 ΔB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChangedEvent {
    pub event_type: SmolStr,
    pub path: String,
    pub old_path: Option<String>,
}

/// Owned wrapper around the running [`Debouncer`].
///
/// Dropping the watcher stops file events. Kept as a separate type so the
/// caller can move it into long-lived application state (`OnceLock`, struct
/// field, ...) without exposing the underlying `Debouncer` API.
pub struct DesktopWatcher {
    debouncer: Debouncer,
}

/// Set up a file system watcher across the supplied desktop source paths.
///
/// `sources` typically contains the User Desktop, the Public Desktop, an
/// optional OneDrive redirect, and an optional user override (the 1.x version
/// pulled this list from `desktop_sources::all_desktop_dirs(custom_ref)`).
///
/// `out` receives one [`FileChangedEvent`] per debounced create / modify /
/// delete. If `out` is dropped the watcher worker exits silently on its next
/// emit.
pub fn setup_file_watcher(
    sources: &[PathBuf],
    out: Sender<FileChangedEvent>,
) -> Result<DesktopWatcher, WatcherError> {
    if sources.is_empty() {
        return Err(WatcherError::NoSources);
    }

    let debouncer = Debouncer::start(Duration::from_millis(200), move |batch| {
        for ev in batch {
            if let Some(payload) = map_event_to_payload(&ev) {
                if out.send(payload).is_err() {
                    // Receiver gone — caller dropped the channel; nothing to do.
                    return;
                }
            }
        }
    })?;

    let mut attached = 0usize;
    for source in sources {
        match debouncer.watch(source, false) {
            Ok(()) => {
                attached += 1;
                tracing::info!("watcher: attached to {}", source.display());
            }
            Err(e) => {
                // Non-fatal: a single source failing (e.g. OneDrive path with
                // permission quirks) should not block the remaining sources.
                tracing::warn!("watcher: failed to attach {}: {e}", source.display());
            }
        }
    }

    if attached == 0 {
        return Err(WatcherError::NoSourcesAttached {
            attempted: sources.len(),
        });
    }

    Ok(DesktopWatcher { debouncer })
}

impl DesktopWatcher {
    /// Add another path to an already-running watcher.
    pub fn watch(&self, path: &Path) -> Result<(), WatcherError> {
        self.debouncer.watch(path, false)?;
        Ok(())
    }

    /// Stop watching `path`.
    pub fn unwatch(&self, path: &Path) -> Result<(), WatcherError> {
        self.debouncer.unwatch(path)?;
        Ok(())
    }
}

/// Convert a debounced event into a frontend-friendly payload.
///
/// Returns `None` for events we deliberately ignore (access-time changes,
/// metadata refresh, anything not classified by the OS as create/modify/
/// delete).
fn map_event_to_payload(event: &DebouncedEvent) -> Option<FileChangedEvent> {
    use notify::EventKind;

    let event_type = match event.kind {
        EventKind::Create(_) => ChangeKind::Create,
        EventKind::Modify(_) => ChangeKind::Modify,
        EventKind::Remove(_) => ChangeKind::Delete,
        _ => return None,
    };

    let path = event.paths.first()?.to_string_lossy().to_string();
    let old_path = event.paths.get(1).map(|p| p.to_string_lossy().to_string());

    Some(FileChangedEvent {
        event_type: SmolStr::new(event_type.as_str()),
        path,
        old_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sources_errors_out() {
        let (tx, _rx) = crossbeam_channel::unbounded();
        let result = setup_file_watcher(&[], tx);
        assert!(matches!(result, Err(WatcherError::NoSources)));
    }

    #[test]
    fn change_kind_as_str_round_trip() {
        assert_eq!(ChangeKind::Create.as_str(), "create");
        assert_eq!(ChangeKind::Modify.as_str(), "modify");
        assert_eq!(ChangeKind::Delete.as_str(), "delete");
    }

    #[test]
    fn file_changed_event_serde_round_trip() {
        let original = FileChangedEvent {
            event_type: SmolStr::new_static("create"),
            path: "/tmp/x.txt".into(),
            old_path: None,
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: FileChangedEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, parsed);
    }
}
