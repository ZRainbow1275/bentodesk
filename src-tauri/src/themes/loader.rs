//! Theme loader — discovers and deserializes JSON theme files.
//!
//! Built-in themes are defined as constants in this module.
//! User themes are loaded from `{app_data}/themes/*.json`.

use std::path::PathBuf;

use super::{Theme, ThemeAnimation, ThemeCapsule, ThemeColors, ThemeGlassmorphism};
use crate::error::BentoDeskError;
use crate::plugins::{PluginRegistry, PluginType};

// ─── Built-in Themes ────────────────────────────────────────

/// Ocean Blue — deep blue glassmorphism theme.
fn ocean_blue() -> Theme {
    Theme {
        id: "ocean-blue".into(),
        name: "Ocean Blue".into(),
        is_builtin: true,
        colors: ThemeColors {
            accent: "#0ea5e9".into(),
            background: "rgba(8, 47, 73, 0.75)".into(),
            text: "#e0f2fe".into(),
            border: "rgba(14, 165, 233, 0.2)".into(),
        },
        capsule: ThemeCapsule {
            shape: "rounded".into(),
            size: "medium".into(),
            blur_radius: 20.0,
        },
        animation: ThemeAnimation {
            expand_duration_ms: 250,
            collapse_duration_ms: 200,
        },
        glassmorphism: ThemeGlassmorphism {
            blur: 20.0,
            opacity: 0.75,
            saturation: 1.6,
        },
    }
}

/// Rose Gold — warm pink/gold glassmorphism theme.
fn rose_gold() -> Theme {
    Theme {
        id: "rose-gold".into(),
        name: "Rose Gold".into(),
        is_builtin: true,
        colors: ThemeColors {
            accent: "#f43f5e".into(),
            background: "rgba(76, 29, 39, 0.75)".into(),
            text: "#fff1f2".into(),
            border: "rgba(244, 63, 94, 0.2)".into(),
        },
        capsule: ThemeCapsule {
            shape: "rounded".into(),
            size: "medium".into(),
            blur_radius: 22.0,
        },
        animation: ThemeAnimation {
            expand_duration_ms: 280,
            collapse_duration_ms: 220,
        },
        glassmorphism: ThemeGlassmorphism {
            blur: 22.0,
            opacity: 0.75,
            saturation: 1.5,
        },
    }
}

/// Forest Green — earthy green glassmorphism theme.
fn forest_green() -> Theme {
    Theme {
        id: "forest-green".into(),
        name: "Forest Green".into(),
        is_builtin: true,
        colors: ThemeColors {
            accent: "#22c55e".into(),
            background: "rgba(20, 46, 26, 0.75)".into(),
            text: "#dcfce7".into(),
            border: "rgba(34, 197, 94, 0.2)".into(),
        },
        capsule: ThemeCapsule {
            shape: "rounded".into(),
            size: "medium".into(),
            blur_radius: 20.0,
        },
        animation: ThemeAnimation {
            expand_duration_ms: 250,
            collapse_duration_ms: 200,
        },
        glassmorphism: ThemeGlassmorphism {
            blur: 20.0,
            opacity: 0.75,
            saturation: 1.5,
        },
    }
}

/// All built-in themes.
fn builtin_themes() -> Vec<Theme> {
    vec![ocean_blue(), rose_gold(), forest_green()]
}

// ─── Disk Loader ────────────────────────────────────────────

/// Resolve the directory where user-installed theme JSON files live.
pub fn themes_dir(handle: &tauri::AppHandle) -> PathBuf {
    let base = tauri::Manager::path(handle)
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("themes")
}

/// Load all themes: built-in first, then user JSON files, then enabled plugin themes.
///
/// User/plugin themes whose ID collides with a built-in are silently skipped.
pub fn load_all_themes(themes_dir: &PathBuf) -> Result<Vec<Theme>, BentoDeskError> {
    let mut themes = builtin_themes();
    let builtin_ids: Vec<String> = themes.iter().map(|t| t.id.clone()).collect();

    // Load user themes from disk (if directory exists)
    if themes_dir.is_dir() {
        let entries = std::fs::read_dir(themes_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                match load_theme_file(&path) {
                    Ok(mut theme) => {
                        // Prevent user themes from claiming built-in status
                        theme.is_builtin = false;
                        // Skip if ID collides with a built-in
                        if builtin_ids.contains(&theme.id) {
                            tracing::warn!(
                                "Skipping user theme '{}' — collides with built-in ID",
                                theme.id
                            );
                            continue;
                        }
                        themes.push(theme);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load theme from {}: {e}",
                            path.display()
                        );
                    }
                }
            }
        }
    }

    // Load themes from enabled plugins.
    // themes_dir is {app_data}/themes, so app_data is its parent.
    if let Some(app_data) = themes_dir.parent() {
        load_plugin_themes(app_data, &mut themes, &builtin_ids);
    }

    Ok(themes)
}

/// Load theme.json from each enabled Theme-type plugin and append to the list.
///
/// Plugin themes that collide with built-in or already-loaded theme IDs are skipped.
fn load_plugin_themes(
    app_data: &std::path::Path,
    themes: &mut Vec<Theme>,
    builtin_ids: &[String],
) {
    let registry = match PluginRegistry::load(app_data) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Cannot load plugin registry for theme discovery: {e}");
            return;
        }
    };

    let existing_ids: Vec<String> = themes.iter().map(|t| t.id.clone()).collect();

    for plugin in &registry.plugins {
        if !plugin.enabled || plugin.plugin_type != PluginType::Theme {
            continue;
        }

        let theme_path = std::path::PathBuf::from(&plugin.install_path).join("theme.json");
        match load_theme_file(&theme_path) {
            Ok(mut theme) => {
                theme.is_builtin = false;
                if builtin_ids.contains(&theme.id) || existing_ids.contains(&theme.id) {
                    tracing::warn!(
                        "Skipping plugin theme '{}' from plugin '{}' — ID collision",
                        theme.id,
                        plugin.id
                    );
                    continue;
                }
                themes.push(theme);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to load theme from plugin '{}': {e}",
                    plugin.id
                );
            }
        }
    }
}

/// Load and deserialize a single theme JSON file.
fn load_theme_file(path: &std::path::Path) -> Result<Theme, BentoDeskError> {
    let content = std::fs::read_to_string(path)?;
    let theme: Theme = serde_json::from_str(&content)?;
    Ok(theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_have_correct_count() {
        let themes = builtin_themes();
        assert_eq!(themes.len(), 3);
    }

    #[test]
    fn builtin_themes_have_unique_ids() {
        let themes = builtin_themes();
        let ids: Vec<&str> = themes.iter().map(|t| t.id.as_str()).collect();
        let mut deduped = ids.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(ids.len(), deduped.len());
    }

    #[test]
    fn builtin_themes_are_marked_builtin() {
        for theme in builtin_themes() {
            assert!(theme.is_builtin, "Theme '{}' should be built-in", theme.id);
        }
    }

    #[test]
    fn ocean_blue_has_expected_values() {
        let theme = ocean_blue();
        assert_eq!(theme.id, "ocean-blue");
        assert_eq!(theme.name, "Ocean Blue");
        assert_eq!(theme.colors.accent, "#0ea5e9");
        assert_eq!(theme.animation.expand_duration_ms, 250);
        assert!((theme.glassmorphism.blur - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn theme_serialization_roundtrip() {
        let theme = ocean_blue();
        let json = serde_json::to_string(&theme).unwrap();
        let parsed: Theme = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, theme.id);
        assert_eq!(parsed.name, theme.name);
        assert_eq!(parsed.colors.accent, theme.colors.accent);
        assert_eq!(
            parsed.animation.expand_duration_ms,
            theme.animation.expand_duration_ms
        );
    }

    #[test]
    fn load_all_themes_with_nonexistent_dir_returns_builtins() {
        let fake_dir = PathBuf::from("/nonexistent/themes/dir");
        let themes = load_all_themes(&fake_dir).unwrap();
        assert_eq!(themes.len(), 3);
    }

    #[test]
    fn load_theme_file_with_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let theme = rose_gold();
        let json = serde_json::to_string_pretty(&theme).unwrap();
        let file_path = dir.path().join("rose-gold.json");
        std::fs::write(&file_path, &json).unwrap();

        let loaded = load_theme_file(&file_path).unwrap();
        assert_eq!(loaded.id, "rose-gold");
        assert_eq!(loaded.name, "Rose Gold");
    }

    #[test]
    fn load_theme_file_with_invalid_json_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("bad.json");
        std::fs::write(&file_path, "{ not valid json }").unwrap();

        let result = load_theme_file(&file_path);
        assert!(result.is_err());
    }

    #[test]
    fn user_themes_loaded_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let custom = Theme {
            id: "custom-purple".into(),
            name: "Custom Purple".into(),
            is_builtin: false,
            colors: ThemeColors {
                accent: "#a855f7".into(),
                background: "rgba(30, 10, 50, 0.8)".into(),
                text: "#f5f3ff".into(),
                border: "rgba(168, 85, 247, 0.2)".into(),
            },
            capsule: ThemeCapsule {
                shape: "rounded".into(),
                size: "medium".into(),
                blur_radius: 18.0,
            },
            animation: ThemeAnimation {
                expand_duration_ms: 200,
                collapse_duration_ms: 180,
            },
            glassmorphism: ThemeGlassmorphism {
                blur: 18.0,
                opacity: 0.8,
                saturation: 1.4,
            },
        };
        let json = serde_json::to_string_pretty(&custom).unwrap();
        std::fs::write(dir.path().join("custom-purple.json"), &json).unwrap();

        let themes = load_all_themes(&dir.path().to_path_buf()).unwrap();
        assert_eq!(themes.len(), 4); // 3 built-in + 1 custom
        let custom_loaded = themes.iter().find(|t| t.id == "custom-purple").unwrap();
        assert_eq!(custom_loaded.name, "Custom Purple");
        assert!(!custom_loaded.is_builtin);
    }

    #[test]
    fn user_theme_with_builtin_id_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        // Create a theme JSON with a built-in ID
        let hijacker = Theme {
            id: "ocean-blue".into(), // collides with built-in
            name: "Fake Ocean".into(),
            is_builtin: false,
            colors: ThemeColors {
                accent: "#ff0000".into(),
                background: "#000000".into(),
                text: "#ffffff".into(),
                border: "#333333".into(),
            },
            capsule: ThemeCapsule {
                shape: "rounded".into(),
                size: "small".into(),
                blur_radius: 10.0,
            },
            animation: ThemeAnimation {
                expand_duration_ms: 100,
                collapse_duration_ms: 100,
            },
            glassmorphism: ThemeGlassmorphism {
                blur: 10.0,
                opacity: 0.5,
                saturation: 1.0,
            },
        };
        let json = serde_json::to_string_pretty(&hijacker).unwrap();
        std::fs::write(dir.path().join("fake-ocean.json"), &json).unwrap();

        let themes = load_all_themes(&dir.path().to_path_buf()).unwrap();
        // Should still be 3 — the hijacker is skipped
        assert_eq!(themes.len(), 3);
        // The ocean-blue should be the built-in one
        let ob = themes.iter().find(|t| t.id == "ocean-blue").unwrap();
        assert_eq!(ob.name, "Ocean Blue");
        assert!(ob.is_builtin);
    }
}
