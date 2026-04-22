use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;

use crate::config::backup;
use crate::config::encryption::EncryptionMode;
use crate::config::migration;
use crate::error::BentoDeskError;
use crate::storage;

/// Visual theme selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
    System,
}

/// Runtime safety profile that controls guardrail limits.
///
/// Higher profiles allow more zones, items, and cache entries at the cost of
/// increased memory usage. Most users should stay on `Balanced`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SafetyProfile {
    Conservative,
    Balanced,
    Expanded,
}

/// Update-check preferences persisted under `settings.updates`.
///
/// These ship with safe defaults so the very first boot still performs a
/// weekly background check without the user opting in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatesConfig {
    #[serde(default = "default_check_frequency")]
    pub check_frequency: UpdateCheckFrequency,
    #[serde(default = "default_true")]
    pub auto_download: bool,
    /// Version string the user asked us to skip (e.g. "1.2.1"). Cleared when
    /// a newer version than the skipped one is published.
    #[serde(default)]
    pub skipped_version: Option<String>,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self {
            check_frequency: UpdateCheckFrequency::Weekly,
            auto_download: true,
            skipped_version: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum UpdateCheckFrequency {
    Daily,
    Weekly,
    Manual,
}

fn default_check_frequency() -> UpdateCheckFrequency {
    UpdateCheckFrequency::Weekly
}

/// Encryption preferences persisted under `settings.encryption`. The
/// passphrase itself is never stored — only the mode — so changing modes
/// always requires a fresh prompt.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EncryptionConfig {
    #[serde(default)]
    pub mode: EncryptionMode,
}

/// v1.2.1 — how zones wake up from their collapsed (capsule) state.
///
/// * `Hover` (default, v1.0–1.2 behaviour): moving the cursor onto the
///   capsule expands the zone after `expand_delay_ms`.
/// * `Always`: zones mount expanded and never auto-collapse — a
///   Stardock-Fences-style persistent layout.
/// * `Click`: hover is inert; a single left-click on the capsule expands
///   the zone. Mouse-leave still auto-collapses, matching launcher UX.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZoneDisplayMode {
    #[default]
    Hover,
    Always,
    Click,
}

/// Application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Numeric schema version. 1.2 introduces the `u32` field; older 1.x
    /// payloads get stamped with the current value during `load_or_default`.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub version: String,
    pub ghost_layer_enabled: bool,
    pub expand_delay_ms: u32,
    pub collapse_delay_ms: u32,
    pub icon_cache_size: u32,
    pub auto_group_enabled: bool,
    pub theme: Theme,
    pub accent_color: String,
    pub desktop_path: String,
    pub watch_paths: Vec<String>,
    pub portable_mode: bool,
    pub launch_at_startup: bool,
    pub show_in_taskbar: bool,
    #[serde(default = "default_safety_profile")]
    pub safety_profile: SafetyProfile,
    /// Active JSON theme ID (e.g. "ocean-blue"). None means use frontend default.
    #[serde(default)]
    pub active_theme: Option<String>,
    /// High-priority startup (Task Scheduler with no delay).
    #[serde(default)]
    pub startup_high_priority: bool,
    /// Enable crash auto-restart via Guardian process.
    #[serde(default)]
    pub crash_restart_enabled: bool,
    /// Maximum crash retries within the crash window before Guardian gives up.
    #[serde(default = "default_crash_max_retries")]
    pub crash_max_retries: u32,
    /// Crash detection window in seconds.
    #[serde(default = "default_crash_window_secs")]
    pub crash_window_secs: u32,
    /// Whether to perform safe recovery actions after hibernate/sleep resume.
    #[serde(default = "default_true")]
    pub safe_start_after_hibernation: bool,
    /// Delay in milliseconds before performing recovery after hibernate resume.
    #[serde(default = "default_hibernate_delay")]
    pub hibernate_resume_delay_ms: u32,
    /// Theme A — update-check preferences. Defaults mean every fresh install
    /// performs a weekly background check with silent auto-download.
    #[serde(default)]
    pub updates: UpdatesConfig,
    /// Theme A — settings encryption choice. Defaults to `None` so existing
    /// plaintext installations keep working without user intervention.
    #[serde(default)]
    pub encryption: EncryptionConfig,
    /// Theme A — downgrade-safe parking lot for fields the current build
    /// does not understand. Migration stashes unknown keys here rather than
    /// dropping them, so a user who rolls back to 1.1 still sees their own
    /// data. `skip_serializing_if` keeps the on-disk file clean when empty.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub _legacy: serde_json::Value,
    /// D1: show debug overlay (hit-rect / anchor / state) in dev/diagnostic mode.
    #[serde(default)]
    pub debug_overlay: bool,
    /// v1.2.1 — zone reveal interaction mode. `Hover` keeps v1.x behaviour.
    #[serde(default)]
    pub zone_display_mode: ZoneDisplayMode,
}

fn default_safety_profile() -> SafetyProfile {
    SafetyProfile::Balanced
}

fn default_schema_version() -> u32 {
    migration::CURRENT_SCHEMA_VERSION
}

fn default_crash_max_retries() -> u32 {
    3
}

fn default_crash_window_secs() -> u32 {
    10
}

fn default_true() -> bool {
    true
}

fn default_hibernate_delay() -> u32 {
    2000
}

/// Partial update for `UpdatesConfig` — every field optional so the UI can
/// send just the toggle the user touched.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdatesConfigUpdate {
    pub check_frequency: Option<UpdateCheckFrequency>,
    pub auto_download: Option<bool>,
    pub skipped_version: Option<Option<String>>,
}

/// Partial update for `EncryptionConfig`. Note the encryption *payload* /
/// passphrase is NOT exposed here — that flows through the dedicated
/// `set_encryption_mode` command so we can run a roundtrip probe before
/// committing the change.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EncryptionConfigUpdate {
    pub mode: Option<EncryptionMode>,
}

/// Partial update for settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsUpdate {
    pub ghost_layer_enabled: Option<bool>,
    pub expand_delay_ms: Option<u32>,
    pub collapse_delay_ms: Option<u32>,
    pub icon_cache_size: Option<u32>,
    pub auto_group_enabled: Option<bool>,
    pub theme: Option<Theme>,
    pub accent_color: Option<String>,
    pub desktop_path: Option<String>,
    pub watch_paths: Option<Vec<String>>,
    pub portable_mode: Option<bool>,
    pub launch_at_startup: Option<bool>,
    pub show_in_taskbar: Option<bool>,
    pub safety_profile: Option<SafetyProfile>,
    pub active_theme: Option<Option<String>>,
    pub startup_high_priority: Option<bool>,
    pub crash_restart_enabled: Option<bool>,
    pub crash_max_retries: Option<u32>,
    pub crash_window_secs: Option<u32>,
    pub safe_start_after_hibernation: Option<bool>,
    pub hibernate_resume_delay_ms: Option<u32>,
    #[serde(default)]
    pub updates: Option<UpdatesConfigUpdate>,
    #[serde(default)]
    pub encryption: Option<EncryptionConfigUpdate>,
    pub debug_overlay: Option<bool>,
    pub zone_display_mode: Option<ZoneDisplayMode>,
}

impl AppSettings {
    /// Load settings from disk, or return sensible defaults.
    ///
    /// Flow:
    /// 1. If the primary file exists, read raw JSON (untyped) so the
    ///    migration dispatcher can see fields the current shape no longer
    ///    knows about and park them in `_legacy`.
    /// 2. Snapshot a rotated `settings.backup.<ts>.json` before rewriting so
    ///    a bad migration can be reverted via
    ///    [`crate::config::backup::restore_backup`].
    /// 3. Deserialize the migrated blob into `AppSettings`.
    /// 4. On any read/parse error, fall through to
    ///    [`storage::read_json_with_recovery`] which pulls from the `.bak`
    ///    sibling written by `write_json_atomic`.
    pub fn load_or_default(handle: &AppHandle) -> Result<Self, BentoDeskError> {
        let path = Self::settings_path(handle);
        Self::load_or_default_from_path(&path)
    }

    fn load_or_default_from_path(path: &PathBuf) -> Result<Self, BentoDeskError> {
        if path.exists() {
            let raw_bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(err) => {
                    tracing::warn!(
                        "Settings file unreadable ({err}); falling back to recovery path"
                    );
                    return Self::load_via_recovery(path);
                }
            };

            let mut value: serde_json::Value = match serde_json::from_slice(&raw_bytes) {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!(
                        "Settings JSON parse failed ({err}); falling back to recovery path"
                    );
                    return Self::load_via_recovery(path);
                }
            };

            if let Err(err) = backup::create_backup(path) {
                tracing::warn!("Pre-migration backup failed: {err}");
            }

            match migration::migrate_in_place(&mut value) {
                Ok(report) if !report.is_noop() => {
                    tracing::info!(
                        "Settings migrated from v{} to v{} ({} step(s))",
                        report.from_version,
                        report.to_version,
                        report.applied_steps.len()
                    );
                    if let Err(err) = storage::write_json_atomic(path, &value) {
                        tracing::error!(
                            "Post-migration write failed: {err}; settings may reload stale"
                        );
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::error!(
                        "Settings migration failed: {err}; leaving file untouched. \
                         Run restore_settings_backup to revert."
                    );
                }
            }

            match serde_json::from_value::<AppSettings>(value) {
                Ok(settings) => return Ok(settings),
                Err(err) => {
                    tracing::error!(
                        "Post-migration deserialize failed: {err}; trying backup recovery"
                    );
                    return Self::load_via_recovery(path);
                }
            }
        }

        Self::load_via_recovery(path)
    }

    fn load_via_recovery(path: &std::path::Path) -> Result<Self, BentoDeskError> {
        match storage::read_json_with_recovery::<AppSettings>(path, "Settings") {
            Ok(Some(settings)) => Ok(settings),
            Ok(None) => Ok(Self::default()),
            Err(e) => {
                tracing::error!(
                    "Settings load failed even after backup recovery, using defaults: {e}"
                );
                Ok(Self::default())
            }
        }
    }

    /// Expose the resolved settings path so the IPC layer (backup/updater
    /// commands) can operate on the same file without duplicating path
    /// resolution logic.
    pub fn path_for(handle: &AppHandle) -> PathBuf {
        Self::settings_path(handle)
    }

    /// Atomically persist settings to disk.
    ///
    /// Writes to a temporary file, flushes, then swaps into place via
    /// [`storage::write_json_atomic`]. The previous primary file is retained as
    /// a `.bak` sibling for crash recovery.
    pub fn save(&self, handle: &AppHandle) -> Result<(), BentoDeskError> {
        let path = Self::settings_path(handle);
        storage::write_json_atomic(&path, self)
    }

    /// Apply a partial update to the settings.
    pub fn apply_update(&mut self, update: SettingsUpdate) {
        if let Some(v) = update.ghost_layer_enabled {
            self.ghost_layer_enabled = v;
        }
        if let Some(v) = update.expand_delay_ms {
            self.expand_delay_ms = v;
        }
        if let Some(v) = update.collapse_delay_ms {
            self.collapse_delay_ms = v;
        }
        if let Some(v) = update.icon_cache_size {
            self.icon_cache_size = v;
        }
        if let Some(v) = update.auto_group_enabled {
            self.auto_group_enabled = v;
        }
        if let Some(v) = update.theme {
            self.theme = v;
        }
        if let Some(v) = update.accent_color {
            self.accent_color = v;
        }
        if let Some(v) = update.desktop_path {
            self.desktop_path = v;
        }
        if let Some(v) = update.watch_paths {
            self.watch_paths = v;
        }
        if let Some(v) = update.portable_mode {
            self.portable_mode = v;
        }
        if let Some(v) = update.launch_at_startup {
            self.launch_at_startup = v;
        }
        if let Some(v) = update.show_in_taskbar {
            self.show_in_taskbar = v;
        }
        if let Some(v) = update.safety_profile {
            self.safety_profile = v;
        }
        if let Some(v) = update.active_theme {
            self.active_theme = v;
        }
        if let Some(v) = update.startup_high_priority {
            self.startup_high_priority = v;
        }
        if let Some(v) = update.crash_restart_enabled {
            self.crash_restart_enabled = v;
        }
        if let Some(v) = update.crash_max_retries {
            self.crash_max_retries = v;
        }
        if let Some(v) = update.crash_window_secs {
            self.crash_window_secs = v;
        }
        if let Some(v) = update.safe_start_after_hibernation {
            self.safe_start_after_hibernation = v;
        }
        if let Some(v) = update.hibernate_resume_delay_ms {
            self.hibernate_resume_delay_ms = v;
        }
        if let Some(v) = update.debug_overlay {
            self.debug_overlay = v;
        }
        if let Some(v) = update.zone_display_mode {
            self.zone_display_mode = v;
        }
        if let Some(upd) = update.updates {
            if let Some(v) = upd.check_frequency {
                self.updates.check_frequency = v;
            }
            if let Some(v) = upd.auto_download {
                self.updates.auto_download = v;
            }
            if let Some(v) = upd.skipped_version {
                self.updates.skipped_version = v;
            }
        }
        if let Some(enc) = update.encryption {
            if let Some(v) = enc.mode {
                self.encryption.mode = v;
            }
        }
    }

    /// Resolve the settings file path.
    fn settings_path(handle: &AppHandle) -> PathBuf {
        let base = Self::data_dir(handle);
        base.join("settings.json")
    }

    /// Determine the data directory (portable or AppData).
    fn data_dir(handle: &AppHandle) -> PathBuf {
        crate::storage::state_data_dir(handle)
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        let desktop_path = dirs::desktop_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        Self {
            schema_version: migration::CURRENT_SCHEMA_VERSION,
            version: "1.0.0".to_string(),
            ghost_layer_enabled: true,
            expand_delay_ms: 150,
            collapse_delay_ms: 400,
            icon_cache_size: 500,
            auto_group_enabled: true,
            theme: Theme::Dark,
            accent_color: "#3b82f6".to_string(),
            desktop_path,
            watch_paths: Vec::new(),
            portable_mode: false,
            launch_at_startup: false,
            show_in_taskbar: false,
            safety_profile: SafetyProfile::Balanced,
            active_theme: None,
            startup_high_priority: false,
            crash_restart_enabled: false,
            crash_max_retries: 3,
            crash_window_secs: 10,
            safe_start_after_hibernation: true,
            hibernate_resume_delay_ms: 2000,
            updates: UpdatesConfig::default(),
            encryption: EncryptionConfig::default(),
            _legacy: serde_json::Value::Null,
            debug_overlay: false,
            zone_display_mode: ZoneDisplayMode::Hover,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_have_expected_values() {
        let settings = AppSettings::default();
        assert_eq!(settings.version, "1.0.0");
        assert!(settings.ghost_layer_enabled);
        assert_eq!(settings.expand_delay_ms, 150);
        assert_eq!(settings.collapse_delay_ms, 400);
        assert_eq!(settings.icon_cache_size, 500);
        assert!(settings.auto_group_enabled);
        assert!(matches!(settings.theme, Theme::Dark));
        assert_eq!(settings.accent_color, "#3b82f6");
        assert!(settings.watch_paths.is_empty());
        assert!(!settings.portable_mode);
        assert!(!settings.launch_at_startup);
        assert!(!settings.show_in_taskbar);
    }

    #[test]
    fn settings_serialization_roundtrip() {
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, settings.version);
        assert_eq!(parsed.icon_cache_size, settings.icon_cache_size);
        assert_eq!(parsed.accent_color, settings.accent_color);
    }

    #[test]
    fn apply_update_changes_specified_fields_only() {
        let mut settings = AppSettings::default();
        let update = SettingsUpdate {
            ghost_layer_enabled: Some(false),
            expand_delay_ms: Some(200),
            collapse_delay_ms: None,
            icon_cache_size: None,
            auto_group_enabled: None,
            theme: Some(Theme::Light),
            accent_color: Some("#ff0000".to_string()),
            desktop_path: None,
            watch_paths: None,
            portable_mode: None,
            launch_at_startup: None,
            show_in_taskbar: None,
            safety_profile: None,
            active_theme: None,
            startup_high_priority: None,
            crash_restart_enabled: None,
            crash_max_retries: None,
            crash_window_secs: None,
            safe_start_after_hibernation: None,
            hibernate_resume_delay_ms: None,
            updates: None,
            encryption: None,
            debug_overlay: None,
            zone_display_mode: None,
        };

        settings.apply_update(update);

        assert!(!settings.ghost_layer_enabled);
        assert_eq!(settings.expand_delay_ms, 200);
        assert_eq!(settings.collapse_delay_ms, 400); // Unchanged
        assert!(matches!(settings.theme, Theme::Light));
        assert_eq!(settings.accent_color, "#ff0000");
        assert_eq!(settings.icon_cache_size, 500); // Unchanged
    }

    #[test]
    fn apply_update_with_no_changes() {
        let mut settings = AppSettings::default();
        let original_accent = settings.accent_color.clone();
        let update = SettingsUpdate {
            ghost_layer_enabled: None,
            expand_delay_ms: None,
            collapse_delay_ms: None,
            icon_cache_size: None,
            auto_group_enabled: None,
            theme: None,
            accent_color: None,
            desktop_path: None,
            watch_paths: None,
            portable_mode: None,
            launch_at_startup: None,
            show_in_taskbar: None,
            safety_profile: None,
            active_theme: None,
            startup_high_priority: None,
            crash_restart_enabled: None,
            crash_max_retries: None,
            crash_window_secs: None,
            safe_start_after_hibernation: None,
            hibernate_resume_delay_ms: None,
            updates: None,
            encryption: None,
            debug_overlay: None,
            zone_display_mode: None,
        };

        settings.apply_update(update);
        assert_eq!(settings.accent_color, original_accent);
    }

    #[test]
    fn zone_display_mode_defaults_to_hover_and_serializes_lowercase() {
        // Default = Hover matches v1.x behaviour — upgrading installs keep
        // their existing interaction model without a schema migration.
        let settings = AppSettings::default();
        assert_eq!(settings.zone_display_mode, ZoneDisplayMode::Hover);

        // Lowercase tag is the wire contract the frontend relies on.
        assert_eq!(
            serde_json::to_string(&ZoneDisplayMode::Always).unwrap(),
            "\"always\""
        );
        assert_eq!(
            serde_json::to_string(&ZoneDisplayMode::Click).unwrap(),
            "\"click\""
        );

        // A legacy v1.2 settings file (no zone_display_mode key) must still
        // deserialize — that is the entire point of the additive default.
        let legacy = r##"{"schema_version":3,"version":"1.2.0","ghost_layer_enabled":true,
            "expand_delay_ms":150,"collapse_delay_ms":400,"icon_cache_size":500,
            "auto_group_enabled":true,"theme":"Dark","accent_color":"#3b82f6",
            "desktop_path":"C:/Users/x/Desktop","watch_paths":[],"portable_mode":false,
            "launch_at_startup":false,"show_in_taskbar":false}"##;
        let parsed: AppSettings = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.zone_display_mode, ZoneDisplayMode::Hover);
    }

    #[test]
    fn apply_update_switches_zone_display_mode() {
        let mut settings = AppSettings::default();
        let update = SettingsUpdate {
            ghost_layer_enabled: None,
            expand_delay_ms: None,
            collapse_delay_ms: None,
            icon_cache_size: None,
            auto_group_enabled: None,
            theme: None,
            accent_color: None,
            desktop_path: None,
            watch_paths: None,
            portable_mode: None,
            launch_at_startup: None,
            show_in_taskbar: None,
            safety_profile: None,
            active_theme: None,
            startup_high_priority: None,
            crash_restart_enabled: None,
            crash_max_retries: None,
            crash_window_secs: None,
            safe_start_after_hibernation: None,
            hibernate_resume_delay_ms: None,
            updates: None,
            encryption: None,
            debug_overlay: None,
            zone_display_mode: Some(ZoneDisplayMode::Always),
        };
        settings.apply_update(update);
        assert_eq!(settings.zone_display_mode, ZoneDisplayMode::Always);
    }

    #[test]
    fn theme_serialization() {
        let dark = serde_json::to_string(&Theme::Dark).unwrap();
        assert_eq!(dark, "\"Dark\"");

        let light = serde_json::to_string(&Theme::Light).unwrap();
        assert_eq!(light, "\"Light\"");

        let system = serde_json::to_string(&Theme::System).unwrap();
        assert_eq!(system, "\"System\"");

        let parsed: Theme = serde_json::from_str("\"Light\"").unwrap();
        assert!(matches!(parsed, Theme::Light));
    }

    #[test]
    fn settings_update_deserialization_with_missing_fields() {
        let json = r#"{"theme": "System"}"#;
        let update: SettingsUpdate = serde_json::from_str(json).unwrap();
        assert!(matches!(update.theme, Some(Theme::System)));
        assert!(update.ghost_layer_enabled.is_none());
        assert!(update.accent_color.is_none());
    }

    #[test]
    fn theme_a_default_has_current_schema_version_and_legacy_null() {
        let settings = AppSettings::default();
        assert_eq!(settings.schema_version, migration::CURRENT_SCHEMA_VERSION);
        assert!(settings._legacy.is_null());
        assert!(matches!(
            settings.updates.check_frequency,
            UpdateCheckFrequency::Weekly
        ));
        assert!(settings.updates.auto_download);
        assert_eq!(settings.encryption.mode, EncryptionMode::None);
    }

    #[test]
    fn theme_a_default_roundtrip_skips_legacy_when_null() {
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        assert!(
            !json.contains("_legacy"),
            "_legacy should be skipped when null, got: {json}"
        );
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert!(parsed._legacy.is_null());
    }

    #[test]
    fn theme_a_load_migrates_v1_payload_and_parks_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let legacy_payload = serde_json::json!({
            "version": "1.1.0",
            "ghost_layer_enabled": true,
            "expand_delay_ms": 150,
            "collapse_delay_ms": 400,
            "icon_cache_size": 500,
            "auto_group_enabled": true,
            "theme": "Dark",
            "accent_color": "#3b82f6",
            "desktop_path": "",
            "watch_paths": [],
            "portable_mode": false,
            "launch_at_startup": false,
            "show_in_taskbar": false,
            "a_dropped_field_from_1_1": { "keep": "me" }
        });
        std::fs::write(&path, serde_json::to_vec(&legacy_payload).unwrap()).unwrap();

        let settings = AppSettings::load_or_default_from_path(&path).unwrap();
        assert_eq!(settings.schema_version, migration::CURRENT_SCHEMA_VERSION);
        assert_eq!(
            settings._legacy["a_dropped_field_from_1_1"]["keep"], "me",
            "unknown legacy fields must be parked in _legacy, not dropped"
        );

        let rotated = backup::list_backups(&path).unwrap();
        assert!(
            !rotated.is_empty(),
            "pre-migration backup must be created before rewriting primary"
        );
    }

    #[test]
    fn theme_a_load_recovers_from_garbage_file_via_bak() {
        // Seed a valid settings.json, then write a .bak sibling by going
        // through write_json_atomic twice, finally corrupt the primary.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        let first = AppSettings::default();
        storage::write_json_atomic(&path, &first).unwrap();
        storage::write_json_atomic(&path, &first).unwrap();

        std::fs::write(&path, b"{ not json").unwrap();

        let recovered = AppSettings::load_or_default_from_path(&path).unwrap();
        assert_eq!(recovered.schema_version, migration::CURRENT_SCHEMA_VERSION);
    }
}
