//! T-085 — JSON Theme loader (lift-verbatim from 1.x `src-tauri/src/themes/`).
//!
//! Manages theme definitions stored as JSON files on disk. Themes carry the
//! 1.x schema verbatim (colors / capsule / animation / glassmorphism) so that
//! existing user-installed `*.json` files in `{app_data}/themes/` continue to
//! load. The `to_theme_tokens()` bridge then converts a parsed [`Theme`] into
//! a [`bentodesk_theme::ThemeTokens`] struct that the renderer consumes.
//!
//! ## Differences from 1.x
//!
//! - **No Tauri.** The 1.x module exposed `#[tauri::command]` wrappers
//!   (`list_themes` / `get_theme` / `get_active_theme` / `set_active_theme`)
//!   that talked to `tauri::State<AppState>` and emitted `theme_changed` via
//!   `tauri::Emitter`. The native port replaces them with plain functions taking
//!   a `&Path` for `themes_dir` and returning `Result`. Active-theme persistence
//!   moves to whoever owns the settings store (caller responsibility, not
//!   ours — see master plan §6 / §11).
//! - **Selected-stack plugin provider.** 1.x called
//!   `PluginRegistry::load(app_data)` to discover plugin-shipped themes.
//!   The native port keeps that data contract for installed or pre-extracted
//!   Theme plugins: enabled registry rows or manifest-discovered plugin
//!   directories provide `theme.json` without a WebView/Tauri bridge.
//! - **Hand-rolled error enum.** Spec §8.1 forbids `thiserror`; [`ThemeError`]
//!   is a plain `enum` with `impl Display + impl core::error::Error`.

pub mod loader;

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

pub use loader::{
    ThemeError, import_theme_file, load_all_themes, load_theme_file, themes_dir, to_theme_tokens,
};

// ─── Theme Schema (byte-for-byte compatible with 1.x JSON files) ────

/// Color palette for a theme.
///
/// Matches the 1.x JSON shape exactly: every color is a CSS-style string
/// (`#rrggbb`, `#rrggbbaa`, `rgb(r, g, b)`, `rgba(r, g, b, a)`). The
/// `to_theme_tokens()` bridge parses these into linear RGBA floats.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeColors {
    pub accent: String,
    pub background: String,
    pub text: String,
    pub border: String,
}

/// Capsule (zone pill) shape configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeCapsule {
    pub shape: String,
    pub size: String,
    pub blur_radius: f64,
}

/// Animation timing configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeAnimation {
    pub expand_duration_ms: u32,
    pub collapse_duration_ms: u32,
}

/// Glassmorphism effect configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeGlassmorphism {
    pub blur: f64,
    pub opacity: f64,
    pub saturation: f64,
}

/// Complete JSON Theme definition.
///
/// Each theme has a unique string ID (kebab-case), a display name, and grouped
/// visual properties. Built-in themes are baked into the binary; user themes
/// are loaded from `{app_data}/themes/*.json`.
///
/// `id` and `name` are `SmolStr` (≤22-byte inline) for hot-path equality.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Theme {
    /// Unique identifier — lowercase kebab-case, e.g. `"ocean-blue"`.
    pub id: SmolStr,
    /// Human-readable display name, e.g. `"Ocean Blue"`.
    pub name: SmolStr,
    /// Whether this theme ships with the app (cannot be deleted).
    #[serde(default)]
    pub is_builtin: bool,
    /// Core color palette.
    pub colors: ThemeColors,
    /// Capsule shape parameters.
    pub capsule: ThemeCapsule,
    /// Animation durations.
    pub animation: ThemeAnimation,
    /// Glassmorphism backdrop-filter settings.
    pub glassmorphism: ThemeGlassmorphism,
}
