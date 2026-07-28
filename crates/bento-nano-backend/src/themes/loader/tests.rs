use super::*;
use crate::plugins::{InstalledPlugin, PluginRegistry, PluginType};

fn plugin_theme(id: &'static str, name: &'static str) -> Theme {
    Theme {
        id: SmolStr::new_static(id),
        name: SmolStr::new_static(name),
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
    }
}

fn scratch_state_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "bento-nano-theme-plugin-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch state");
    dir
}

fn write_theme_json(path: &Path, theme: &Theme) {
    let json = serde_json::to_string_pretty(theme).expect("theme json");
    std::fs::write(path, json).expect("write theme");
}

fn write_registry_plugin(
    state_dir: &Path,
    plugin_id: &str,
    theme: &Theme,
    enabled: bool,
) -> PathBuf {
    let plugin_dir = state_dir.join("plugins").join(plugin_id);
    std::fs::create_dir_all(&plugin_dir).expect("plugin dir");
    write_theme_json(&plugin_dir.join("theme.json"), theme);
    let mut registry = PluginRegistry::default();
    registry.plugins.push(InstalledPlugin {
        id: plugin_id.to_owned(),
        name: "Plugin Theme".into(),
        version: "1.0.0".into(),
        plugin_type: PluginType::Theme,
        author: "Tester".into(),
        description: "Registry-backed theme plugin".into(),
        enabled,
        installed_at: "2026-01-01T00:00:00.000Z".into(),
        install_path: plugin_dir.to_string_lossy().into_owned(),
    });
    registry.save(state_dir).expect("save registry");
    plugin_dir
}

fn write_preextracted_plugin(state_dir: &Path, plugin_id: &str, theme: &Theme) -> PathBuf {
    let plugin_dir = state_dir.join("plugins").join(plugin_id);
    std::fs::create_dir_all(&plugin_dir).expect("plugin dir");
    let manifest = serde_json::json!({
        "id": plugin_id,
        "name": "Pre-extracted Theme",
        "version": "1.0.0",
        "type": "theme",
        "author": "Tester",
        "description": "Manifest-backed theme plugin",
        "min_app_version": null,
        "icon": null
    });
    std::fs::write(
        plugin_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("manifest json"),
    )
    .expect("write manifest");
    write_theme_json(&plugin_dir.join("theme.json"), theme);
    plugin_dir
}

#[test]
fn builtin_themes_have_correct_count() {
    assert_eq!(builtin_themes().len(), 3);
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
        assert!(theme.is_builtin, "theme '{}' should be built-in", theme.id);
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
    let json = serde_json::to_string(&theme).expect("serialize");
    let parsed: Theme = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.id, theme.id);
    assert_eq!(parsed.colors.accent, theme.colors.accent);
    assert_eq!(
        parsed.animation.expand_duration_ms,
        theme.animation.expand_duration_ms
    );
}

#[test]
fn load_all_themes_with_nonexistent_dir_returns_builtins() {
    let fake_dir = PathBuf::from("/nonexistent/themes/dir");
    let themes = load_all_themes(&fake_dir).expect("builtins");
    assert_eq!(themes.len(), 3);
}

#[test]
fn import_theme_file_copies_valid_json_into_themes_dir() {
    let state_dir = scratch_state_dir("import-valid");
    let source_dir = state_dir.join("source");
    std::fs::create_dir_all(&source_dir).expect("source dir");
    let source_theme = plugin_theme("imported-cyan", "Imported Cyan");
    let source_path = source_dir.join("downloaded-theme.json");
    write_theme_json(&source_path, &source_theme);

    let themes_dir = state_dir.join("themes");
    let imported = import_theme_file(&source_path, &themes_dir).expect("import theme");
    assert_eq!(imported.id, "imported-cyan");
    assert!(!imported.is_builtin);
    assert!(themes_dir.join("imported-cyan.json").is_file());

    let loaded = load_all_themes(&themes_dir).expect("load themes after import");
    assert!(loaded.iter().any(|theme| theme.id == "imported-cyan"));

    let _ = std::fs::remove_dir_all(&state_dir);
}

#[test]
fn import_theme_file_rejects_builtin_id_collision() {
    let state_dir = scratch_state_dir("import-built-in-collision");
    let source_path = state_dir.join("ocean.json");
    let mut hijacker = ocean_blue();
    hijacker.is_builtin = false;
    hijacker.name = SmolStr::new_static("Fake Ocean");
    write_theme_json(&source_path, &hijacker);

    assert!(matches!(
        import_theme_file(&source_path, &state_dir.join("themes")),
        Err(ThemeError::BuiltinCollision { id }) if id == "ocean-blue"
    ));

    let _ = std::fs::remove_dir_all(&state_dir);
}

#[test]
fn enabled_registry_theme_plugin_loads_without_themes_dir() {
    let state_dir = scratch_state_dir("registry-enabled");
    let theme = plugin_theme("plugin-purple", "Plugin Purple");
    write_registry_plugin(&state_dir, "com.test.plugin-purple", &theme, true);

    let themes = load_all_themes(&state_dir.join("themes")).expect("load plugin themes");
    let loaded = themes
        .iter()
        .find(|theme| theme.id == "plugin-purple")
        .expect("plugin theme visible");
    assert_eq!(loaded.name, "Plugin Purple");
    assert!(!loaded.is_builtin);

    let _ = std::fs::remove_dir_all(&state_dir);
}

#[test]
fn disabled_registry_theme_plugin_is_skipped() {
    let state_dir = scratch_state_dir("registry-disabled");
    let theme = plugin_theme("disabled-purple", "Disabled Purple");
    write_registry_plugin(&state_dir, "com.test.disabled-purple", &theme, false);

    let themes = load_all_themes(&state_dir.join("themes")).expect("load plugin themes");
    assert!(!themes.iter().any(|theme| theme.id == "disabled-purple"));

    let _ = std::fs::remove_dir_all(&state_dir);
}

#[test]
fn tampered_registry_path_cannot_redirect_theme_loading() {
    let state_dir = scratch_state_dir("registry-path-tamper");
    let outside_dir = state_dir.join("outside-plugin-data");
    std::fs::create_dir_all(&outside_dir).expect("outside dir");
    write_theme_json(
        &outside_dir.join("theme.json"),
        &plugin_theme("redirected-purple", "Redirected Purple"),
    );
    let mut registry = PluginRegistry::default();
    registry.plugins.push(InstalledPlugin {
        id: "com.test.redirected-purple".into(),
        name: "Redirected Theme".into(),
        version: "1.0.0".into(),
        plugin_type: PluginType::Theme,
        author: "Tester".into(),
        description: "Tampered registry path".into(),
        enabled: true,
        installed_at: "2026-01-01T00:00:00.000Z".into(),
        install_path: outside_dir.to_string_lossy().into_owned(),
    });
    registry.save(&state_dir).expect("save registry");

    let themes = load_all_themes(&state_dir.join("themes")).expect("load themes");
    assert!(
        !themes.iter().any(|theme| theme.id == "redirected-purple"),
        "theme discovery must use state/plugins/<validated-id>, not persisted install_path"
    );

    let _ = std::fs::remove_dir_all(&state_dir);
}

#[test]
fn preextracted_manifest_theme_plugin_is_loaded() {
    let state_dir = scratch_state_dir("preextracted");
    let theme = plugin_theme("manifest-purple", "Manifest Purple");
    write_preextracted_plugin(&state_dir, "com.test.manifest-purple", &theme);

    let themes = load_all_themes(&state_dir.join("themes")).expect("load plugin themes");
    assert!(themes.iter().any(|theme| theme.id == "manifest-purple"));

    let _ = std::fs::remove_dir_all(&state_dir);
}

#[test]
fn parse_hex_rgb() {
    let c = parse_color("#0ea5e9", "test").expect("parse");
    assert_eq!(c, Color::from_u8(0x0e, 0xa5, 0xe9, 0xFF));
}

#[test]
fn parse_hex_rgba() {
    let c = parse_color("#18181CCC", "test").expect("parse");
    assert_eq!(c, Color::from_u8(0x18, 0x18, 0x1C, 0xCC));
}

#[test]
fn parse_rgba_decimal() {
    let c = parse_color("rgba(8, 47, 73, 0.75)", "test").expect("parse");
    assert_eq!(c.r, 8.0 / 255.0);
    assert_eq!(c.a, 0.75);
}

#[test]
fn parse_rgb_decimal() {
    let c = parse_color("rgb(8, 47, 73)", "test").expect("parse");
    assert_eq!(c.r, 8.0 / 255.0);
    assert_eq!(c.a, 1.0);
}

#[test]
fn parse_invalid_color_errors() {
    assert!(matches!(
        parse_color("notacolor", "test"),
        Err(ThemeError::InvalidColor { .. })
    ));
    assert!(matches!(
        parse_color("#ZZ", "test"),
        Err(ThemeError::InvalidColor { .. })
    ));
}

#[test]
fn to_theme_tokens_round_trip_ocean_blue() {
    let tokens = to_theme_tokens(&ocean_blue()).expect("convert");
    // Accent comes through unchanged: #0ea5e9
    assert_eq!(
        tokens.palette.accent,
        Color::from_u8(0x0e, 0xa5, 0xe9, 0xFF)
    );
    assert!(tokens.palette.surface.a > 0.0);
    assert_eq!(tokens.typo.font_family.as_str(), "Segoe UI");
}

#[test]
fn user_theme_with_builtin_id_is_skipped() {
    let dir = std::env::temp_dir().join(format!("bento-nano-themes-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let hijacker = Theme {
        id: SmolStr::new_static("ocean-blue"),
        name: SmolStr::new_static("Fake Ocean"),
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
    let json = serde_json::to_string_pretty(&hijacker).expect("serialize");
    let file = dir.join("fake-ocean.json");
    std::fs::write(&file, &json).expect("write");

    let themes = load_all_themes(&dir).expect("load");
    assert_eq!(themes.len(), 3, "hijacker should be dropped");
    let ob = themes
        .iter()
        .find(|t| t.id == "ocean-blue")
        .expect("present");
    assert_eq!(ob.name, "Ocean Blue");
    assert!(ob.is_builtin);

    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_dir(&dir);
}
