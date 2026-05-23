//! Legacy Tauri `settings.json` compatibility importer.
//!
//! The selected-stack runtime persists settings in [`super::Vault`], but
//! BentoDesk 1.x wrote user preferences to a sibling `settings.json`. This
//! module performs a conservative one-way import: it only writes selected-stack
//! keys that are still absent from the vault, skips values whose wire format is
//! no longer supported, and never imports passphrases or other secret material.

use std::path::{Path, PathBuf};

use serde_json::Value;
use smol_str::SmolStr;

use super::{EncryptionMode, SettingValue, Vault, VaultError};

/// Stable vault key for the active JSON theme id.
pub const KEY_ACTIVE_THEME: &str = "active_theme";
/// Stable vault key for the update check cadence.
pub const KEY_UPDATES_CHECK_FREQUENCY: &str = "updates.check_frequency";
/// Stable vault key for update auto-download preference.
pub const KEY_UPDATES_AUTO_DOWNLOAD: &str = "updates.auto_download";
/// Stable vault key for skipped update version.
pub const KEY_UPDATES_SKIPPED_VERSION: &str = "updates.skipped_version";
/// Stable vault key for settings encryption mode.
pub const KEY_ENCRYPTION_MODE: &str = "encryption.mode";
/// Stable vault key for theme accent swatch.
pub const KEY_THEME_BASE_ACCENT: &str = "theme.base_accent";
/// Stable vault key for diagnostic overlay visibility.
pub const KEY_DEBUG_OVERLAY: &str = "debug_overlay";
/// Stable vault key for global zone display mode.
pub const KEY_ZONE_DISPLAY_MODE: &str = "zone_display_mode";

const LEGACY_SETTINGS_FILE: &str = "settings.json";
const LEGACY_DIR_NAMES: &[&str] = &[
    "BentoDesk",
    "BentoDesk-Dev",
    "BentoDesk-dev",
    "com.bentodesk.app",
    "com.bentodesk.app-dev",
];

/// Outcome of a Tauri settings import pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TauriSettingsMigrationReport {
    /// Existing source file selected for the import, or `None` when none of
    /// the known BentoDesk 1.x locations exists.
    pub source_path: Option<PathBuf>,
    /// Vault keys imported during this pass.
    pub imported_keys: Vec<SmolStr>,
    /// Vault keys skipped because the selected-stack vault already owns them.
    pub skipped_existing_keys: Vec<SmolStr>,
    /// Legacy fields skipped because their value shape or enum variant was invalid.
    pub skipped_invalid_fields: Vec<SmolStr>,
    /// Legacy fields intentionally not imported by this compatibility slice.
    pub skipped_unsupported_fields: Vec<SmolStr>,
    /// `true` when the vault is passphrase-locked and therefore cannot accept writes.
    pub vault_locked: bool,
}

impl TauriSettingsMigrationReport {
    fn missing_source() -> Self {
        Self {
            source_path: None,
            imported_keys: Vec::new(),
            skipped_existing_keys: Vec::new(),
            skipped_invalid_fields: Vec::new(),
            skipped_unsupported_fields: Vec::new(),
            vault_locked: false,
        }
    }

    fn for_source(source_path: PathBuf) -> Self {
        Self {
            source_path: Some(source_path),
            imported_keys: Vec::new(),
            skipped_existing_keys: Vec::new(),
            skipped_invalid_fields: Vec::new(),
            skipped_unsupported_fields: Vec::new(),
            vault_locked: false,
        }
    }

    fn locked(source_path: PathBuf) -> Self {
        let mut report = Self::for_source(source_path);
        report.vault_locked = true;
        report
    }

    /// Returns `true` when this pass changed the vault.
    pub fn imported_any(&self) -> bool {
        !self.imported_keys.is_empty()
    }

    /// Returns `true` when a source existed but every supported key was
    /// already present in the selected-stack vault.
    pub fn only_skipped_existing(&self) -> bool {
        self.source_path.is_some()
            && self.imported_keys.is_empty()
            && self.skipped_invalid_fields.is_empty()
            && self.skipped_unsupported_fields.is_empty()
            && !self.skipped_existing_keys.is_empty()
    }
}

/// Errors surfaced by the legacy settings importer.
#[derive(Debug)]
pub enum TauriSettingsMigrationError {
    /// Filesystem metadata/read failed for the selected source file.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The selected source file exceeds the backend JSON safety cap.
    LimitExceeded {
        path: PathBuf,
        bytes: u64,
        max_bytes: u64,
    },
    /// The selected source file is not valid JSON.
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// Writing imported keys into the selected-stack vault failed.
    Vault(VaultError),
}

impl core::fmt::Display for TauriSettingsMigrationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    f,
                    "legacy Tauri settings I/O failed at {}: {source}",
                    path.display()
                )
            }
            Self::LimitExceeded {
                path,
                bytes,
                max_bytes,
            } => write!(
                f,
                "legacy Tauri settings file exceeds safety limit at {}: {bytes} > {max_bytes}",
                path.display()
            ),
            Self::Json { path, source } => {
                write!(
                    f,
                    "legacy Tauri settings JSON invalid at {}: {source}",
                    path.display()
                )
            }
            Self::Vault(source) => write!(f, "legacy Tauri settings vault write failed: {source}"),
        }
    }
}

impl core::error::Error for TauriSettingsMigrationError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::Vault(source) => Some(source),
            Self::LimitExceeded { .. } => None,
        }
    }
}

impl From<VaultError> for TauriSettingsMigrationError {
    fn from(value: VaultError) -> Self {
        Self::Vault(value)
    }
}

/// Build the known BentoDesk 1.x `settings.json` candidate list for a selected-stack state dir.
pub fn legacy_settings_candidates(state_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    push_unique(&mut candidates, state_dir.join(LEGACY_SETTINGS_FILE));

    if let Some(parent) = state_dir.parent() {
        for dirname in LEGACY_DIR_NAMES {
            push_unique(
                &mut candidates,
                parent.join(dirname).join(LEGACY_SETTINGS_FILE),
            );
        }
    }

    candidates
}

/// Import the first existing BentoDesk 1.x `settings.json` into the selected-stack vault.
pub fn migrate_first_existing_tauri_settings(
    state_dir: &Path,
    vault: &mut Vault,
    accepted_accent_hex: &[&str],
) -> Result<TauriSettingsMigrationReport, TauriSettingsMigrationError> {
    let Some(source_path) = legacy_settings_candidates(state_dir)
        .into_iter()
        .find(|path| path.is_file())
    else {
        return Ok(TauriSettingsMigrationReport::missing_source());
    };

    if vault.is_locked_passphrase() {
        return Ok(TauriSettingsMigrationReport::locked(source_path));
    }

    migrate_tauri_settings_file(&source_path, vault, accepted_accent_hex)
}

/// Import one explicit BentoDesk 1.x `settings.json` into the selected-stack vault.
pub fn migrate_tauri_settings_file(
    source_path: &Path,
    vault: &mut Vault,
    accepted_accent_hex: &[&str],
) -> Result<TauriSettingsMigrationReport, TauriSettingsMigrationError> {
    let bytes = read_legacy_settings(source_path)?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|source| TauriSettingsMigrationError::Json {
            path: source_path.to_path_buf(),
            source,
        })?;

    let mut report = TauriSettingsMigrationReport::for_source(source_path.to_path_buf());
    migrate_from_value(&value, vault, accepted_accent_hex, &mut report)?;
    if report.imported_any() {
        vault.flush()?;
    }
    Ok(report)
}

fn push_unique(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    if candidates.iter().any(|existing| existing == &path) {
        return;
    }
    candidates.push(path);
}

fn read_legacy_settings(source_path: &Path) -> Result<Vec<u8>, TauriSettingsMigrationError> {
    let metadata =
        std::fs::metadata(source_path).map_err(|source| TauriSettingsMigrationError::Io {
            path: source_path.to_path_buf(),
            source,
        })?;
    let max_bytes = crate::storage::MAX_JSON_STATE_BYTES;
    if metadata.len() > max_bytes {
        return Err(TauriSettingsMigrationError::LimitExceeded {
            path: source_path.to_path_buf(),
            bytes: metadata.len(),
            max_bytes,
        });
    }
    std::fs::read(source_path).map_err(|source| TauriSettingsMigrationError::Io {
        path: source_path.to_path_buf(),
        source,
    })
}

fn migrate_from_value(
    value: &Value,
    vault: &mut Vault,
    accepted_accent_hex: &[&str],
    report: &mut TauriSettingsMigrationReport,
) -> Result<(), TauriSettingsMigrationError> {
    let Some(object) = value.as_object() else {
        report
            .skipped_invalid_fields
            .push(SmolStr::new_static("<root>"));
        return Ok(());
    };

    migrate_active_theme(object.get("active_theme"), vault, report);
    migrate_accent_color(
        object.get("accent_color"),
        vault,
        accepted_accent_hex,
        report,
    );
    migrate_updates(object.get("updates"), vault, report);
    migrate_encryption(object.get("encryption"), vault, report)?;
    migrate_bool_field(
        object.get("debug_overlay"),
        KEY_DEBUG_OVERLAY,
        "debug_overlay",
        vault,
        report,
    );
    migrate_zone_display_mode(object.get("zone_display_mode"), vault, report);

    if object.contains_key("theme") {
        report
            .skipped_unsupported_fields
            .push(SmolStr::new_static("theme"));
    }

    Ok(())
}

fn migrate_active_theme(
    value: Option<&Value>,
    vault: &mut Vault,
    report: &mut TauriSettingsMigrationReport,
) {
    if let Some(theme_id) = optional_non_empty_string(value, "active_theme", report) {
        insert_if_absent(vault, KEY_ACTIVE_THEME, SettingValue::Str(theme_id), report);
    }
}

fn migrate_accent_color(
    value: Option<&Value>,
    vault: &mut Vault,
    accepted_accent_hex: &[&str],
    report: &mut TauriSettingsMigrationReport,
) {
    let Some(accent) = optional_non_empty_string(value, "accent_color", report) else {
        return;
    };
    let Some(canonical) = accepted_accent_hex
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(accent.as_str()))
        .copied()
    else {
        report
            .skipped_invalid_fields
            .push(SmolStr::new_static("accent_color"));
        return;
    };
    insert_if_absent(
        vault,
        KEY_THEME_BASE_ACCENT,
        SettingValue::Str(SmolStr::new(canonical)),
        report,
    );
}

fn migrate_updates(
    value: Option<&Value>,
    vault: &mut Vault,
    report: &mut TauriSettingsMigrationReport,
) {
    let Some(updates) = value.and_then(Value::as_object) else {
        if value.is_some() {
            report
                .skipped_invalid_fields
                .push(SmolStr::new_static("updates"));
        }
        return;
    };

    if let Some(frequency) = optional_non_empty_string(
        updates.get("check_frequency"),
        "updates.check_frequency",
        report,
    ) {
        if matches!(frequency.as_str(), "Daily" | "Weekly" | "Manual") {
            insert_if_absent(
                vault,
                KEY_UPDATES_CHECK_FREQUENCY,
                SettingValue::Str(frequency),
                report,
            );
        } else {
            report
                .skipped_invalid_fields
                .push(SmolStr::new_static("updates.check_frequency"));
        }
    }

    migrate_bool_field(
        updates.get("auto_download"),
        KEY_UPDATES_AUTO_DOWNLOAD,
        "updates.auto_download",
        vault,
        report,
    );

    if let Some(version_value) = updates.get("skipped_version") {
        if version_value.is_null() {
            return;
        }
        if let Some(version) =
            optional_non_empty_string(Some(version_value), "updates.skipped_version", report)
        {
            insert_if_absent(
                vault,
                KEY_UPDATES_SKIPPED_VERSION,
                SettingValue::Str(version),
                report,
            );
        }
    }
}

fn migrate_encryption(
    value: Option<&Value>,
    vault: &mut Vault,
    report: &mut TauriSettingsMigrationReport,
) -> Result<(), TauriSettingsMigrationError> {
    let Some(encryption) = value.and_then(Value::as_object) else {
        if value.is_some() {
            report
                .skipped_invalid_fields
                .push(SmolStr::new_static("encryption"));
        }
        return Ok(());
    };
    let Some(mode) = optional_non_empty_string(encryption.get("mode"), "encryption.mode", report)
    else {
        return Ok(());
    };

    match mode.as_str() {
        "None" => {
            if insert_if_absent(
                vault,
                KEY_ENCRYPTION_MODE,
                SettingValue::Str(SmolStr::new_static("None")),
                report,
            ) {
                vault.set_mode(EncryptionMode::None)?;
            }
        }
        "Dpapi" => {
            if insert_if_absent(
                vault,
                KEY_ENCRYPTION_MODE,
                SettingValue::Str(SmolStr::new_static("Dpapi")),
                report,
            ) {
                vault.set_mode(EncryptionMode::Dpapi)?;
            }
        }
        "Passphrase" => {
            report
                .skipped_unsupported_fields
                .push(SmolStr::new_static("encryption.mode:Passphrase"));
        }
        _ => {
            report
                .skipped_invalid_fields
                .push(SmolStr::new_static("encryption.mode"));
        }
    }
    Ok(())
}

fn migrate_bool_field(
    value: Option<&Value>,
    key: &'static str,
    field: &'static str,
    vault: &mut Vault,
    report: &mut TauriSettingsMigrationReport,
) {
    let Some(value) = value else {
        return;
    };
    let Some(bool_value) = value.as_bool() else {
        report
            .skipped_invalid_fields
            .push(SmolStr::new_static(field));
        return;
    };
    insert_if_absent(vault, key, SettingValue::Bool(bool_value), report);
}

fn migrate_zone_display_mode(
    value: Option<&Value>,
    vault: &mut Vault,
    report: &mut TauriSettingsMigrationReport,
) {
    let Some(mode) = optional_non_empty_string(value, "zone_display_mode", report) else {
        return;
    };
    if matches!(mode.as_str(), "hover" | "always" | "click") {
        insert_if_absent(
            vault,
            KEY_ZONE_DISPLAY_MODE,
            SettingValue::Str(mode),
            report,
        );
    } else {
        report
            .skipped_invalid_fields
            .push(SmolStr::new_static("zone_display_mode"));
    }
}

fn optional_non_empty_string(
    value: Option<&Value>,
    field: &'static str,
    report: &mut TauriSettingsMigrationReport,
) -> Option<SmolStr> {
    match value {
        Some(Value::String(text)) if !text.trim().is_empty() => Some(SmolStr::new(text.as_str())),
        Some(Value::String(_)) => {
            report
                .skipped_invalid_fields
                .push(SmolStr::new_static(field));
            None
        }
        Some(Value::Null) | None => None,
        Some(_) => {
            report
                .skipped_invalid_fields
                .push(SmolStr::new_static(field));
            None
        }
    }
}

fn insert_if_absent(
    vault: &mut Vault,
    key: &'static str,
    value: SettingValue,
    report: &mut TauriSettingsMigrationReport,
) -> bool {
    if vault.get_setting(key).is_some() {
        report.skipped_existing_keys.push(SmolStr::new_static(key));
        return false;
    }
    vault.set_setting(key, value);
    report.imported_keys.push(SmolStr::new_static(key));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_vault::wire::ModeTag;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tempdir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "bentonano-tauri-settings-{label}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create tempdir");
        dir
    }

    fn write_legacy_settings(path: &Path, body: &str) {
        std::fs::write(path, body).expect("write legacy settings");
    }

    #[test]
    fn candidate_list_prefers_selected_state_dir_settings() {
        let base = PathBuf::from(r"C:\Users\Test\AppData\Roaming\BentoDesk");
        let candidates = legacy_settings_candidates(&base);
        assert_eq!(candidates.first(), Some(&base.join("settings.json")));
        assert!(candidates.contains(&PathBuf::from(
            r"C:\Users\Test\AppData\Roaming\com.bentodesk.app\settings.json"
        )));
    }

    #[test]
    fn imports_supported_settings_without_overwriting_existing_keys() {
        let dir = tempdir("import");
        let settings_path = dir.join("settings.json");
        let vault_path = dir.join("vault.bin");
        write_legacy_settings(
            &settings_path,
            r##"{
  "theme": "Light",
  "accent_color": "#3b82f6",
  "active_theme": "ocean-blue",
  "updates": {
    "check_frequency": "Daily",
    "auto_download": true,
    "skipped_version": "2.0.0"
  },
  "encryption": { "mode": "None" },
  "debug_overlay": true,
  "zone_display_mode": "click"
}"##,
        );

        let mut vault = Vault::open(&vault_path).expect("open vault");
        vault.set_setting(KEY_UPDATES_AUTO_DOWNLOAD, SettingValue::Bool(false));
        vault.flush().expect("seed existing");

        let report =
            migrate_first_existing_tauri_settings(&dir, &mut vault, &["#3b82f6"]).expect("migrate");

        assert!(
            report
                .imported_keys
                .contains(&SmolStr::new_static(KEY_ACTIVE_THEME))
        );
        assert!(
            report
                .skipped_existing_keys
                .contains(&SmolStr::new_static(KEY_UPDATES_AUTO_DOWNLOAD))
        );
        assert!(
            report
                .skipped_unsupported_fields
                .contains(&SmolStr::new_static("theme"))
        );
        assert_eq!(
            vault.get_setting(KEY_ACTIVE_THEME),
            Some(SettingValue::Str(SmolStr::new_static("ocean-blue")))
        );
        assert_eq!(
            vault.get_setting(KEY_THEME_BASE_ACCENT),
            Some(SettingValue::Str(SmolStr::new_static("#3b82f6")))
        );
        assert_eq!(
            vault.get_setting(KEY_UPDATES_CHECK_FREQUENCY),
            Some(SettingValue::Str(SmolStr::new_static("Daily")))
        );
        assert_eq!(
            vault.get_setting(KEY_UPDATES_AUTO_DOWNLOAD),
            Some(SettingValue::Bool(false))
        );
        assert_eq!(
            vault.get_setting(KEY_UPDATES_SKIPPED_VERSION),
            Some(SettingValue::Str(SmolStr::new_static("2.0.0")))
        );
        assert_eq!(
            vault.get_setting(KEY_ENCRYPTION_MODE),
            Some(SettingValue::Str(SmolStr::new_static("None")))
        );
        assert_eq!(
            vault.get_setting(KEY_DEBUG_OVERLAY),
            Some(SettingValue::Bool(true))
        );
        assert_eq!(
            vault.get_setting(KEY_ZONE_DISPLAY_MODE),
            Some(SettingValue::Str(SmolStr::new_static("click")))
        );

        let reopened = Vault::open(&vault_path).expect("reopen");
        assert_eq!(
            reopened.get_setting(KEY_ACTIVE_THEME),
            Some(SettingValue::Str(SmolStr::new_static("ocean-blue")))
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_and_unsupported_legacy_values_are_skipped() {
        let dir = tempdir("invalid");
        let settings_path = dir.join("settings.json");
        let vault_path = dir.join("vault.bin");
        write_legacy_settings(
            &settings_path,
            r##"{
  "accent_color": "#ff0000",
  "active_theme": "",
  "updates": {
    "check_frequency": "Hourly",
    "auto_download": "yes",
    "skipped_version": ""
  },
  "encryption": { "mode": "Passphrase" },
  "debug_overlay": "true",
  "zone_display_mode": "popup"
}"##,
        );

        let mut vault = Vault::open(&vault_path).expect("open vault");
        let report = migrate_tauri_settings_file(&settings_path, &mut vault, &["#3b82f6"])
            .expect("migrate invalid");

        assert!(report.imported_keys.is_empty());
        assert!(
            report
                .skipped_invalid_fields
                .contains(&SmolStr::new_static("accent_color"))
        );
        assert!(
            report
                .skipped_invalid_fields
                .contains(&SmolStr::new_static("updates.check_frequency"))
        );
        assert!(
            report
                .skipped_invalid_fields
                .contains(&SmolStr::new_static("debug_overlay"))
        );
        assert!(
            report
                .skipped_unsupported_fields
                .contains(&SmolStr::new_static("encryption.mode:Passphrase"))
        );
        assert_eq!(vault.get_setting(KEY_ACTIVE_THEME), None);
        assert_eq!(vault.get_setting(KEY_ENCRYPTION_MODE), None);
        assert_eq!(vault.mode_tag(), ModeTag::None);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_source_is_noop() {
        let dir = tempdir("missing");
        let vault_path = dir.join("vault.bin");
        let mut vault = Vault::open(&vault_path).expect("open vault");
        let report = migrate_first_existing_tauri_settings(&dir, &mut vault, &["#3b82f6"])
            .expect("missing source");

        assert_eq!(report.source_path, None);
        assert!(!report.imported_any());
        assert!(!vault.is_dirty());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_mode_import_updates_real_vault_mode() {
        let dir = tempdir("dpapi");
        let settings_path = dir.join("settings.json");
        let vault_path = dir.join("vault.bin");
        write_legacy_settings(&settings_path, r#"{ "encryption": { "mode": "Dpapi" } }"#);

        let mut vault = Vault::open(&vault_path).expect("open vault");
        let report =
            migrate_tauri_settings_file(&settings_path, &mut vault, &[]).expect("migrate dpapi");

        assert!(
            report
                .imported_keys
                .contains(&SmolStr::new_static(KEY_ENCRYPTION_MODE))
        );
        assert_eq!(vault.mode_tag(), ModeTag::Dpapi);
        let reopened = Vault::open(&vault_path).expect("reopen dpapi vault");
        assert_eq!(reopened.mode_tag(), ModeTag::Dpapi);
        assert_eq!(
            reopened.get_setting(KEY_ENCRYPTION_MODE),
            Some(SettingValue::Str(SmolStr::new_static("Dpapi")))
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
