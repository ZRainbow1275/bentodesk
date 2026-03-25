//! Watches the user's Desktop directory for file changes using `notify` v7.
//!
//! Events are debounced (200 ms) and forwarded to the frontend via Tauri events.

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebouncedEvent};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::BentoDeskError;

/// Payload emitted to the frontend when desktop files change.
#[derive(Debug, Clone, Serialize)]
pub struct FileChangedPayload {
    pub event_type: String,
    pub path: String,
    pub old_path: Option<String>,
}

/// Set up a file system watcher on the user's Desktop directory.
///
/// Uses `notify` v9 with debouncing to avoid flooding the frontend with events
/// during rapid file operations (e.g., extracting a ZIP).
pub fn setup_file_watcher(handle: &AppHandle) -> Result<(), BentoDeskError> {
    let desktop_path = dirs::desktop_dir()
        .ok_or_else(|| BentoDeskError::ConfigError("Cannot determine Desktop path".into()))?;

    let handle_clone = handle.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(200),
        None,
        move |events: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| {
            match events {
                Ok(debounced_events) => {
                    for event in debounced_events {
                        if let Some(payload) = map_event_to_payload(&event) {
                            if let Err(e) = handle_clone.emit("file_changed", &payload) {
                                tracing::warn!("Failed to emit file_changed event: {}", e);
                            }
                        }
                    }
                }
                Err(errors) => {
                    for e in errors {
                        tracing::warn!("File watcher error: {}", e);
                    }
                }
            }
        },
    )?;

    debouncer.watch(&desktop_path, RecursiveMode::NonRecursive)?;

    // Store the debouncer in managed state so it is not dropped
    handle.manage(WatcherState {
        _debouncer: std::sync::Mutex::new(Some(debouncer)),
    });

    tracing::info!(
        "Desktop file watcher started on: {}",
        desktop_path.display()
    );
    Ok(())
}

/// Convert a debounced notify event into a frontend-friendly payload.
fn map_event_to_payload(event: &DebouncedEvent) -> Option<FileChangedPayload> {
    use notify::EventKind;

    let kind = &event.event.kind;
    let paths = &event.event.paths;

    let event_type = match kind {
        EventKind::Create(_) => "create",
        EventKind::Modify(_) => "modify",
        EventKind::Remove(_) => "delete",
        _ => return None,
    };

    let path = paths.first()?.to_string_lossy().to_string();
    let old_path = if paths.len() > 1 {
        Some(paths[1].to_string_lossy().to_string())
    } else {
        None
    };

    Some(FileChangedPayload {
        event_type: event_type.to_string(),
        path,
        old_path,
    })
}

/// State wrapper to keep the debouncer alive for the application lifetime.
pub struct WatcherState {
    _debouncer: std::sync::Mutex<
        Option<
            notify_debouncer_full::Debouncer<
                RecommendedWatcher,
                notify_debouncer_full::FileIdMap,
            >,
        >,
    >,
}
