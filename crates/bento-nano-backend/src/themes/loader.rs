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
mod tests;
