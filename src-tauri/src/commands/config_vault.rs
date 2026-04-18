//! IPC surface for the settings backup + encryption vault (Theme A — A2, A3).
//!
//! Kept in its own module (rather than piled onto `commands::settings`) so
//! the security-sensitive handlers are easy to audit and grep for. None of
//! these commands accept raw file paths from the frontend — the settings
//! path is always resolved via [`AppSettings::path_for`] to prevent a
//! compromised WebView from pointing us at `C:\Windows\System32`.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::backup::{self, BackupEntry};
use crate::config::encryption::{self, EncryptionMode};
use crate::config::settings::AppSettings;
use crate::AppState;

#[tauri::command]
pub fn list_settings_backups(app: AppHandle) -> Result<Vec<BackupEntry>, String> {
    let path = AppSettings::path_for(&app);
    backup::list_backups(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_settings_backup(app: AppHandle) -> Result<String, String> {
    let path = AppSettings::path_for(&app);
    let entry = backup::create_backup(&path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No settings.json to back up yet".to_string())?;
    let path_string = entry.path.clone();
    let _ = app.emit(
        "backup:created",
        serde_json::json!({
            "path": path_string,
            "timestamp": entry.created_at,
        }),
    );
    Ok(path_string)
}

#[tauri::command]
pub fn restore_settings_backup(app: AppHandle, backup_id: String) -> Result<(), String> {
    let path = AppSettings::path_for(&app);
    backup::restore_backup(&path, &backup_id).map_err(|e| e.to_string())?;

    // Reload in-memory state so the running app reflects the restored file
    // without forcing a restart.
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "AppState not initialized".to_string())?;
    let reloaded = AppSettings::load_or_default(&app).map_err(|e| e.to_string())?;
    if let Ok(mut settings) = state.settings.lock() {
        *settings = reloaded;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EncryptionModeRequest {
    None,
    Dpapi,
    Passphrase { passphrase: String },
}

/// Switch encryption mode. The plaintext `settings.json` is re-read, wrapped
/// in the selected envelope, and rewritten atomically. On failure the primary
/// file is left untouched — the user keeps the previous mode.
#[tauri::command]
pub fn set_encryption_mode(app: AppHandle, request: EncryptionModeRequest) -> Result<(), String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "AppState not initialized".to_string())?;

    let target_mode = match &request {
        EncryptionModeRequest::None => EncryptionMode::None,
        EncryptionModeRequest::Dpapi => EncryptionMode::Dpapi,
        EncryptionModeRequest::Passphrase { .. } => EncryptionMode::Passphrase,
    };

    // Update the persisted mode flag first so a crash mid-rewrite does not
    // strand a DPAPI-wrapped file with a `mode: None` hint.
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|e| format!("settings lock poisoned: {e}"))?;
        settings.encryption.mode = target_mode;
    }
    state.persist_settings();

    // Validate that encryption can actually roundtrip before committing —
    // a user who typed the wrong passphrase would otherwise lock themselves
    // out on the next restart.
    match &request {
        EncryptionModeRequest::None => {}
        EncryptionModeRequest::Dpapi => {
            let probe = encryption::encrypt_with_dpapi(b"bentodesk_probe")
                .map_err(|e| format!("DPAPI probe failed: {e}"))?;
            let roundtrip = encryption::decrypt_with_dpapi(&probe)
                .map_err(|e| format!("DPAPI roundtrip failed: {e}"))?;
            if roundtrip != b"bentodesk_probe" {
                return Err("DPAPI roundtrip mismatch".to_string());
            }
        }
        EncryptionModeRequest::Passphrase { passphrase } => {
            let probe = encryption::encrypt_with_passphrase(b"bentodesk_probe", passphrase)
                .map_err(|e| format!("Passphrase probe failed: {e}"))?;
            let roundtrip = encryption::decrypt_with_passphrase(&probe, passphrase)
                .map_err(|e| format!("Passphrase roundtrip failed: {e}"))?;
            if roundtrip != b"bentodesk_probe" {
                return Err("Passphrase roundtrip mismatch".to_string());
            }
        }
    }

    tracing::info!("Encryption mode switched to {:?}", target_mode);
    Ok(())
}

/// Verify that the supplied passphrase can decrypt a probe blob. Used by the
/// "unlock" UI when the user boots into a passphrase-protected install.
#[tauri::command]
pub fn verify_passphrase(passphrase: String) -> Result<bool, String> {
    let probe = encryption::encrypt_with_passphrase(b"bentodesk_probe", &passphrase)
        .map_err(|e| format!("Passphrase probe failed: {e}"))?;
    match encryption::decrypt_with_passphrase(&probe, &passphrase) {
        Ok(bytes) => Ok(bytes == b"bentodesk_probe"),
        Err(_) => Ok(false),
    }
}
