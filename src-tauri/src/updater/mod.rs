//! Tauri v2 updater integration (Theme A — A1).
//!
//! Wraps [`tauri_plugin_updater`] in BentoDesk-flavoured commands so the
//! frontend can drive the update lifecycle with its own UI (we disable the
//! built-in dialog via `plugins.updater.dialog = false`).
//!
//! State machine (owned entirely by the plugin; we mirror progress to the
//! frontend):
//!
//! ```text
//!     check_for_updates ──► Option<UpdateInfo>
//!                            │
//!                            ▼ (if Some)
//!                         download_update ──► "update:progress" events
//!                            │
//!                            ▼
//!                 install_update_and_restart
//! ```
//!
//! `on_before_exit` in `lib.rs` drains the recovery bundle + restores hidden
//! files *before* the NSIS installer kills the process, so a mid-update
//! crash cannot leave ghost icons in `.bentodesk/`.

pub mod event;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::config::settings::AppSettings;
use crate::error::BentoDeskError;
use crate::AppState;

/// Thin DTO returned to the frontend when a new version is available.
/// Keeps the `tauri_plugin_updater::Update` type internal so we don't leak
/// the plugin's shape into the IPC surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub date: Option<String>,
    pub body: Option<String>,
}

/// Check the configured endpoint for an available update. Returns `None` when
/// the user is on the latest version or when they previously asked to skip
/// the incoming version (stored in `settings.updates.skipped_version`).
pub async fn check_for_updates(app: AppHandle) -> Result<Option<UpdateInfo>, BentoDeskError> {
    let updater = app
        .updater()
        .map_err(|e| BentoDeskError::ConfigError(format!("Updater init failed: {e}")))?;
    let current_version = app.package_info().version.to_string();

    let pending = updater
        .check()
        .await
        .map_err(|e| BentoDeskError::ConfigError(format!("Update check failed: {e}")))?;

    match pending {
        Some(update) => {
            let version = update.version.clone();

            if let Some(state) = app.try_state::<AppState>() {
                let skipped = {
                    let settings = state.settings.lock().map_err(|e| {
                        BentoDeskError::ConfigError(format!("settings lock poisoned: {e}"))
                    })?;
                    settings.updates.skipped_version.clone()
                };
                if skipped.as_deref() == Some(version.as_str()) {
                    tracing::info!("Update {version} available but user asked to skip it");
                    return Ok(None);
                }
            }

            Ok(Some(UpdateInfo {
                version,
                current_version,
                date: update.date.map(|d| d.to_string()),
                body: update.body.clone(),
            }))
        }
        None => Ok(None),
    }
}

/// Download the pending update, emitting `update:progress` + `update:ready`
/// events. The plugin keeps the downloaded bytes in a temp dir and hands them
/// back to us as a byte buffer we pass to `install`.
pub async fn download_update(app: AppHandle) -> Result<(), BentoDeskError> {
    let updater = app
        .updater()
        .map_err(|e| BentoDeskError::ConfigError(format!("Updater init failed: {e}")))?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|e| BentoDeskError::ConfigError(format!("Update check failed: {e}")))?
    else {
        return Err(BentoDeskError::ConfigError(
            "No update available to download".to_string(),
        ));
    };

    let app_for_progress = app.clone();
    let app_for_done = app.clone();

    update
        .download_and_install(
            move |chunk_len, content_length| {
                let _ = app_for_progress.emit(
                    "update:progress",
                    event::ProgressPayload {
                        chunk_len: chunk_len as u64,
                        total_bytes: content_length,
                    },
                );
            },
            move || {
                let _ = app_for_done.emit("update:ready", ());
            },
        )
        .await
        .map_err(|e| {
            let _ = app.emit(
                "update:error",
                event::ErrorPayload {
                    kind: "download".to_string(),
                    message: e.to_string(),
                },
            );
            BentoDeskError::ConfigError(format!("Download / install failed: {e}"))
        })?;

    Ok(())
}

/// After `download_update` resolves successfully, the plugin has already
/// staged the installer and the usual flow is to restart. The Tauri runtime
/// will drive `on_before_exit` for us so the recovery bundle runs before
/// the installer replaces the binary.
pub async fn install_update_and_restart(app: AppHandle) -> Result<(), BentoDeskError> {
    app.restart();
}

/// Persist `settings.updates.skipped_version = Some(version)` so the next
/// `check_for_updates` call returns `None` for that specific build.
pub fn skip_update_version(app: &AppHandle, version: String) -> Result<(), BentoDeskError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| BentoDeskError::ConfigError("AppState not yet initialized".to_string()))?;
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|e| BentoDeskError::ConfigError(format!("settings lock poisoned: {e}")))?;
        settings.updates.skipped_version = Some(version.clone());
    }
    state.persist_settings();
    tracing::info!("User requested to skip update {version}");
    Ok(())
}

/// Fire-and-forget helper used by tray wiring: run `check_for_updates` in the
/// background and emit `update:available` when a non-skipped build is found.
pub fn spawn_background_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        match check_for_updates(app.clone()).await {
            Ok(Some(info)) => {
                let _ = app.emit(
                    "update:available",
                    serde_json::json!({
                        "version": info.version,
                        "current_version": info.current_version,
                        "body": info.body,
                    }),
                );
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!("Background update check failed: {err}");
            }
        }
    });
}

/// Tell the updater module whether to respect the `check_frequency` setting.
/// Returns the hours-between-checks suggested by the persisted preference.
pub fn check_interval_hours(settings: &AppSettings) -> Option<u64> {
    use crate::config::settings::UpdateCheckFrequency;
    match settings.updates.check_frequency {
        UpdateCheckFrequency::Daily => Some(24),
        UpdateCheckFrequency::Weekly => Some(24 * 7),
        UpdateCheckFrequency::Manual => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::UpdateCheckFrequency;

    #[test]
    fn check_interval_matches_frequency() {
        let mut settings = AppSettings::default();
        settings.updates.check_frequency = UpdateCheckFrequency::Daily;
        assert_eq!(check_interval_hours(&settings), Some(24));

        settings.updates.check_frequency = UpdateCheckFrequency::Weekly;
        assert_eq!(check_interval_hours(&settings), Some(24 * 7));

        settings.updates.check_frequency = UpdateCheckFrequency::Manual;
        assert_eq!(check_interval_hours(&settings), None);
    }
}
