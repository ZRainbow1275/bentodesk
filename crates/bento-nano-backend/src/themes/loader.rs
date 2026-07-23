//! Theme loader — discovers and deserializes JSON theme files, then bridges
//! them to [`bento_nano_theme::ThemeTokens`] for the renderer.
//!
//! Three baked themes ship in the binary (`ocean-blue`, `rose-gold`,
//! `forest-green`) — identical color values to 1.x so existing user themes
//! that override one ID get the same visual baseline. User-installed JSON
//! lives at `{app_data}/themes/*.json` and is merged on top, with collisions
//! against built-in IDs silently rejected (built-in wins). Enabled Theme
//! plugins contribute `<state_dir>/plugins/<validated-id>/theme.json` through
//! the same `PluginRegistry` lifecycle as 1.x. Persisted absolute paths are
//! metadata only and are never trusted for filesystem access.

use std::path::{Path, PathBuf};

use bento_nano_style::Color;
use bento_nano_theme::typo::{FontSizes, FontWeights, LineHeights};
use bento_nano_theme::{PaletteTokens, ThemeTokens, TypoTokens, radius, shadow, spacing};
use smol_str::SmolStr;

use crate::plugins::{PluginRegistry, PluginType, install_path_for};

use super::{Theme, ThemeAnimation, ThemeCapsule, ThemeColors, ThemeGlassmorphism};

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ─────────────

/// Errors surfaced by the theme loader.
///
/// Carries enough structure for the renderer to render a meaningful message
/// without exposing raw `std::io` / `serde_json` types across the public API
/// (the latter would force every downstream caller to add the same deps).
#[derive(Debug)]
pub enum ThemeError {
    /// Failed to read a theme file from disk.
    Io { path: PathBuf, message: String },
    /// JSON deserialization failure.
    Parse { path: PathBuf, message: String },
    /// A color string did not match `#rrggbb` / `#rrggbbaa` / `rgb()` / `rgba()`.
    InvalidColor { field: &'static str, value: String },
    /// No theme matched the requested ID.
    NotFound { id: String },
    /// A user/imported theme tried to override a built-in theme ID.
    BuiltinCollision { id: String },
    /// A theme ID cannot be converted into a safe on-disk file name.
    InvalidThemeId { id: String },
    /// The requested import source is not an acceptable theme JSON file.
    Import { path: PathBuf, message: String },
}

impl core::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(f, "theme io error at {}: {}", path.display(), message)
            }
            Self::Parse { path, message } => {
                write!(f, "theme parse error at {}: {}", path.display(), message)
            }
            Self::InvalidColor { field, value } => {
                write!(f, "invalid color in '{}': {}", field, value)
            }
            Self::NotFound { id } => write!(f, "theme not found: {}", id),
            Self::BuiltinCollision { id } => {
                write!(f, "theme id '{}' collides with a built-in theme", id)
            }
            Self::InvalidThemeId { id } => write!(f, "invalid theme id: {}", id),
            Self::Import { path, message } => {
                write!(f, "theme import error at {}: {}", path.display(), message)
            }
        }
    }
}

impl core::error::Error for ThemeError {}

// ─── Built-in Themes (color values 1:1 with 1.x) ────────────────────

fn ocean_blue() -> Theme {
    Theme {
        id: SmolStr::new_static("ocean-blue"),
        name: SmolStr::new_static("Ocean Blue"),
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

fn rose_gold() -> Theme {
    Theme {
        id: SmolStr::new_static("rose-gold"),
        name: SmolStr::new_static("Rose Gold"),
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

fn forest_green() -> Theme {
    Theme {
        id: SmolStr::new_static("forest-green"),
        name: SmolStr::new_static("Forest Green"),
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

/// Every built-in theme. The renderer iterates this slice to populate any
/// theme-picker UI without round-tripping through disk.
fn builtin_themes() -> Vec<Theme> {
    vec![ocean_blue(), rose_gold(), forest_green()]
}

// ─── Disk Loader ─────────────────────────────────────────────────────

/// Resolve the directory where user-installed theme JSON files live.
///
/// The 1.x signature was `themes_dir(handle: &tauri::AppHandle) -> PathBuf`
/// which delegated to `crate::storage::state_data_dir(handle).join("themes")`.
/// In nano, the caller (typically `bento-nano-app::storage`) owns app-data
/// path resolution and passes the resolved directory in.
pub fn themes_dir(app_data: &Path) -> PathBuf {
    app_data.join("themes")
}

/// Load all themes: built-in first, then user JSON files.
///
/// User themes whose ID collides with a built-in are silently dropped. A
/// missing or non-existent `themes_dir` is **not** an error — the function
/// returns the built-in trio.
pub fn load_all_themes(themes_dir: &Path) -> Result<Vec<Theme>, ThemeError> {
    let mut themes = builtin_themes();
    let builtin_ids: Vec<SmolStr> = themes.iter().map(|t| t.id.clone()).collect();

    if !themes_dir.is_dir() {
        if let Some(state_dir) = themes_dir.parent() {
            load_plugin_themes(state_dir, &mut themes);
        }
        return Ok(themes);
    }

    let entries = std::fs::read_dir(themes_dir).map_err(|e| ThemeError::Io {
        path: themes_dir.to_path_buf(),
        message: e.to_string(),
    })?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("themes: read_dir entry error: {e}");
                continue;
            }
        };
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        match load_theme_file(&path) {
            Ok(mut theme) => {
                theme.is_builtin = false;
                if builtin_ids.contains(&theme.id) {
                    tracing::warn!(
                        "themes: skipping user theme '{}' — collides with built-in ID",
                        theme.id
                    );
                    continue;
                }
                themes.push(theme);
            }
            Err(e) => {
                tracing::warn!("themes: failed to load {}: {e}", path.display());
            }
        }
    }

    if let Some(state_dir) = themes_dir.parent() {
        load_plugin_themes(state_dir, &mut themes);
    }

    Ok(themes)
}

/// Validate and copy one user-selected JSON theme into the selected-stack
/// themes directory.
///
/// This is the backend half of the native Settings import UX. It deliberately
/// does not accept raw JSON strings or sample payloads: the caller must pass a
/// real filesystem path returned by a native picker or an equivalent test
/// fixture, and the function copies bytes into `{app_data}/themes/<id>.json`.
pub fn import_theme_file(source_path: &Path, themes_dir: &Path) -> Result<Theme, ThemeError> {
    if !source_path.is_file() {
        return Err(ThemeError::Import {
            path: source_path.to_path_buf(),
            message: "source is not a file".to_owned(),
        });
    }
    if !path_has_json_extension(source_path) {
        return Err(ThemeError::Import {
            path: source_path.to_path_buf(),
            message: "source must be a .json theme file".to_owned(),
        });
    }

    let mut theme = load_theme_file(source_path)?;
    theme.is_builtin = false;
    to_theme_tokens(&theme)?;
    if builtin_themes()
        .iter()
        .any(|builtin| builtin.id == theme.id)
    {
        return Err(ThemeError::BuiltinCollision {
            id: theme.id.to_string(),
        });
    }

    let Some(file_name) = imported_theme_file_name(theme.id.as_str()) else {
        return Err(ThemeError::InvalidThemeId {
            id: theme.id.to_string(),
        });
    };

    std::fs::create_dir_all(themes_dir).map_err(|e| ThemeError::Io {
        path: themes_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let destination = themes_dir.join(file_name);
    let source_canonical = source_path.canonicalize().ok();
    let destination_canonical = destination.canonicalize().ok();
    let source_is_destination =
        source_canonical.is_some() && source_canonical == destination_canonical;

    if !source_is_destination {
        let bytes = std::fs::read(source_path).map_err(|e| ThemeError::Io {
            path: source_path.to_path_buf(),
            message: e.to_string(),
        })?;
        let temp_destination = destination.with_extension("json.importing");
        std::fs::write(&temp_destination, bytes).map_err(|e| ThemeError::Io {
            path: temp_destination.clone(),
            message: e.to_string(),
        })?;
        if destination.exists() {
            std::fs::remove_file(&destination).map_err(|e| ThemeError::Io {
                path: destination.clone(),
                message: e.to_string(),
            })?;
        }
        if let Err(e) = std::fs::rename(&temp_destination, &destination) {
            let _ = std::fs::remove_file(&temp_destination);
            return Err(ThemeError::Io {
                path: destination,
                message: e.to_string(),
            });
        }
    }

    let mut imported = load_theme_file(&destination)?;
    imported.is_builtin = false;
    to_theme_tokens(&imported)?;
    Ok(imported)
}

fn path_has_json_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

fn imported_theme_file_name(id: &str) -> Option<String> {
    if id.is_empty() || id.len() > 128 {
        return None;
    }
    if !id
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_'))
    {
        return None;
    }
    Some(format!("{id}.json"))
}

fn load_plugin_themes(state_dir: &Path, themes: &mut Vec<Theme>) {
    let registry = match PluginRegistry::load(state_dir) {
        Ok(registry) => registry,
        Err(error) => {
            tracing::warn!("themes: cannot load plugin registry for theme discovery: {error}");
            return;
        }
    };

    for plugin in &registry.plugins {
        if !plugin.enabled || plugin.plugin_type != PluginType::Theme {
            continue;
        }

        // Registry JSON is user-writable state. Derive the owned directory from
        // the validated ID instead of following a persisted absolute path.
        let plugin_dir = match install_path_for(state_dir, &plugin.id) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(
                    "themes: skipping plugin '{}' with invalid registry id: {error}",
                    plugin.id
                );
                continue;
            }
        };
        let theme_path = plugin_dir.join("theme.json");
        match load_theme_file(&theme_path) {
            Ok(mut theme) => {
                theme.is_builtin = false;
                if themes.iter().any(|existing| existing.id == theme.id) {
                    tracing::warn!(
                        "themes: skipping plugin theme '{}' from plugin '{}' - ID collision",
                        theme.id,
                        plugin.id
                    );
                    continue;
                }
                themes.push(theme);
            }
            Err(error) => {
                tracing::warn!(
                    "themes: failed to load theme from plugin '{}': {error}",
                    plugin.id
                );
            }
        }
    }
}

/// Load and deserialize a single theme JSON file.
pub fn load_theme_file(path: &Path) -> Result<Theme, ThemeError> {
    let content = std::fs::read_to_string(path).map_err(|e| ThemeError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    serde_json::from_str(&content).map_err(|e| ThemeError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

// ─── ThemeTokens bridge ──────────────────────────────────────────────

/// Convert a JSON [`Theme`] into a renderer-ready [`ThemeTokens`].
///
/// The 1.x JSON schema only carries 4 colors (`accent` / `background` / `text`
/// / `border`); the nano [`PaletteTokens`] has 16 slots. We derive the missing
/// slots by darkening / lightening the four canonical colors, matching the
/// hand-tuned dark-default ratios used in `bento_nano_theme::palette::DARK`.
///
/// Spacing / radius / shadow / typography use the in-tree defaults — JSON
/// themes are color-only by design, identical to 1.x semantics.
pub fn to_theme_tokens(theme: &Theme) -> Result<ThemeTokens, ThemeError> {
    let accent = parse_color(&theme.colors.accent, "colors.accent")?;
    let bg = parse_color(&theme.colors.background, "colors.background")?;
    let text = parse_color(&theme.colors.text, "colors.text")?;
    let border = parse_color(&theme.colors.border, "colors.border")?;

    let palette = PaletteTokens {
        bg,
        // Surface uses the supplied background's RGB but with the glassmorphism
        // opacity baked in (matches 1.x BentoCard 0xCC alpha intent).
        surface: with_alpha(bg, (theme.glassmorphism.opacity as f32).clamp(0.0, 1.0)),
        surface_alt: brighten(bg, 0.05),
        border,
        text,
        text_muted: with_alpha(text, 0.6),
        accent,
        accent_hover: brighten(accent, 0.1),
        danger: Color::from_u8(0xE5, 0x4B, 0x4B, 0xFF),
        success: Color::from_u8(0x3F, 0xB9, 0x50, 0xFF),
        warning: Color::from_u8(0xE3, 0xA0, 0x08, 0xFF),
        info: Color::from_u8(0x33, 0x99, 0xCC, 0xFF),
        scrim: Color::from_u8(0x00, 0x00, 0x00, 0x80),
        hover_overlay: with_alpha(text, 0.08),
        active_overlay: with_alpha(text, 0.16),
        selection: with_alpha(accent, 0.4),
    };

    Ok(ThemeTokens {
        palette,
        spacing: spacing::DEFAULT,
        radius: radius::DEFAULT,
        shadow: shadow::DEFAULT,
        typo: TypoTokens {
            font_family: SmolStr::new_static("Segoe UI"),
            sizes: FontSizes {
                xs: 11.0,
                sm: 13.0,
                md: 16.0,
                lg: 20.0,
                xl: 24.0,
                xxl: 32.0,
            },
            weights: FontWeights {
                normal: 400,
                medium: 500,
                bold: 700,
            },
            line_heights: LineHeights {
                tight: 1.1,
                normal: 1.4,
                loose: 1.7,
            },
        },
    })
}

// ─── Color string parser ─────────────────────────────────────────────
//
// Accepts the four CSS-ish forms the 1.x JSON theme files actually use:
//   #rrggbb           -> alpha = 0xFF
//   #rrggbbaa         -> alpha = aa
//   rgb(r, g, b)      -> alpha = 0xFF, components 0..=255
//   rgba(r, g, b, a)  -> alpha = a, components 0..=255 / a 0.0..=1.0

fn parse_color(s: &str, field: &'static str) -> Result<Color, ThemeError> {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix('#') {
        return parse_hex(rest, field, s);
    }
    if let Some(inner) = trimmed
        .strip_prefix("rgb(")
        .and_then(|x| x.strip_suffix(')'))
    {
        return parse_rgb(inner, field, s, false);
    }
    if let Some(inner) = trimmed
        .strip_prefix("rgba(")
        .and_then(|x| x.strip_suffix(')'))
    {
        return parse_rgb(inner, field, s, true);
    }
    Err(ThemeError::InvalidColor {
        field,
        value: s.to_string(),
    })
}

fn parse_hex(rest: &str, field: &'static str, original: &str) -> Result<Color, ThemeError> {
    let bad = || ThemeError::InvalidColor {
        field,
        value: original.to_string(),
    };
    match rest.len() {
        6 => {
            let r = u8::from_str_radix(&rest[0..2], 16).map_err(|_| bad())?;
            let g = u8::from_str_radix(&rest[2..4], 16).map_err(|_| bad())?;
            let b = u8::from_str_radix(&rest[4..6], 16).map_err(|_| bad())?;
            Ok(Color::from_u8(r, g, b, 0xFF))
        }
        8 => {
            let r = u8::from_str_radix(&rest[0..2], 16).map_err(|_| bad())?;
            let g = u8::from_str_radix(&rest[2..4], 16).map_err(|_| bad())?;
            let b = u8::from_str_radix(&rest[4..6], 16).map_err(|_| bad())?;
            let a = u8::from_str_radix(&rest[6..8], 16).map_err(|_| bad())?;
            Ok(Color::from_u8(r, g, b, a))
        }
        _ => Err(bad()),
    }
}

fn parse_rgb(
    inner: &str,
    field: &'static str,
    original: &str,
    has_alpha: bool,
) -> Result<Color, ThemeError> {
    let bad = || ThemeError::InvalidColor {
        field,
        value: original.to_string(),
    };
    let parts: smallvec::SmallVec<[&str; 4]> = inner.split(',').map(str::trim).collect();
    let expected = if has_alpha { 4 } else { 3 };
    if parts.len() != expected {
        return Err(bad());
    }
    let r: u16 = parts[0].parse().map_err(|_| bad())?;
    let g: u16 = parts[1].parse().map_err(|_| bad())?;
    let b: u16 = parts[2].parse().map_err(|_| bad())?;
    if r > 255 || g > 255 || b > 255 {
        return Err(bad());
    }
    let a_f = if has_alpha {
        let a: f32 = parts[3].parse().map_err(|_| bad())?;
        a.clamp(0.0, 1.0)
    } else {
        1.0
    };
    Ok(Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a_f,
    })
}

// ─── Color math (kept here, no extra dep) ────────────────────────────

fn with_alpha(c: Color, a: f32) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a: a.clamp(0.0, 1.0),
    }
}

fn brighten(c: Color, amount: f32) -> Color {
    let a = amount.clamp(0.0, 1.0);
    Color {
        r: (c.r + (1.0 - c.r) * a).clamp(0.0, 1.0),
        g: (c.g + (1.0 - c.g) * a).clamp(0.0, 1.0),
        b: (c.b + (1.0 - c.b) * a).clamp(0.0, 1.0),
        a: c.a,
    }
}

#[cfg(test)]
mod tests {
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
        let dir =
            std::env::temp_dir().join(format!("bento-nano-themes-test-{}", std::process::id()));
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
}
