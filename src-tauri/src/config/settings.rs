use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;

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

/// Application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
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
}

fn default_safety_profile() -> SafetyProfile {
    SafetyProfile::Balanced
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
}

impl AppSettings {
    /// Load settings from disk, or return sensible defaults.
    ///
    /// Uses [`storage::read_json_with_recovery`] so that a corrupt primary file
    /// is automatically healed from the `.bak` sibling created by prior saves.
    pub fn load_or_default(handle: &AppHandle) -> Result<Self, BentoDeskError> {
        let path = Self::settings_path(handle);
        match storage::read_json_with_recovery::<AppSettings>(&path, "Settings") {
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
    }

    /// Resolve the settings file path.
    fn settings_path(handle: &AppHandle) -> PathBuf {
        let base = Self::data_dir(handle);
        base.join("settings.json")
    }

    /// Determine the data directory (portable or AppData).
    fn data_dir(handle: &AppHandle) -> PathBuf {
        if let Ok(exe_path) = std::env::current_exe() {
            let portable_dir = exe_path.parent().map(|p| p.join("data"));
            if let Some(ref dir) = portable_dir {
                if dir.exists() {
                    return dir.clone();
                }
            }
        }
        tauri::Manager::path(handle)
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        let desktop_path = dirs::desktop_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        Self {
            version: "1.0.0".to_string(),
            ghost_layer_enabled: true,
            expand_delay_ms: 150,
            collapse_delay_ms: 300,
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
        assert_eq!(settings.collapse_delay_ms, 300);
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
        };

        settings.apply_update(update);

        assert!(!settings.ghost_layer_enabled);
        assert_eq!(settings.expand_delay_ms, 200);
        assert_eq!(settings.collapse_delay_ms, 300); // Unchanged
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
        };

        settings.apply_update(update);
        assert_eq!(settings.accent_color, original_accent);
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
}
