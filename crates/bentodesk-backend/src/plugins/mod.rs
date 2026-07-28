//! T-096 — `.bdplugin` plugin runtime (manifest + registry + lifecycle).
//!
//! Plugins are ZIP archives containing a `manifest.json` and type-specific
//! assets (currently only `Theme` plugins). Installed plugins are tracked
//! in `<state_dir>/plugins/registry.json` via the same atomic-write +
//! backup-recovery system as layout/settings ([`crate::storage`]).
//!
//! ## v0.0.x scope (Q7 ruling 2026-05-03)
//!
//! Plugin runtime. Registry / manifest validation / archive install /
//! uninstall / toggle_enabled all ship as selected-stack backend services.
//! [`loader::install_from_zip`] extracts `.bdplugin` / `.zip` archives into
//! `<state_dir>/plugins/<id>/` with zip-slip, duplicate-entry, size, symlink,
//! manifest, and theme-payload validation before updating the registry.
//!
//! ## What changed vs 1.x (uniform across submodules)
//!
//! - **Spec §8.1**: hand-rolled [`PluginError`] replaces
//!   `BentoDeskError::PluginError`.
//! - **Spec §8 (Q1 corollary)**: `chrono::Utc::now().to_rfc3339()` →
//!   [`crate::time::now_rfc3339`] in [`loader::build_record`].
//! - **Spec §8 dependency narrowing**: `uuid` (tmp-dir naming) remains
//!   eliminated; archive extraction uses the workspace `zip` crate with
//!   default features disabled and deflate support only.
//! - **Tauri removal**: every entrypoint takes `state_dir: &Path` directly
//!   instead of resolving from `tauri::AppHandle`.
//! - **ΔB ruling**: `#[derive(Serialize, Deserialize)]` on every public
//!   DTO (`PluginManifest`, `PluginType`, `PluginRegistry`,
//!   `InstalledPlugin`) for v2.x scripting forward-compat.

pub mod loader;
pub mod manifest;
pub mod registry;

pub use loader::{build_record, install_from_zip, install_path_for, toggle_enabled, uninstall};
pub use manifest::{PluginManifest, PluginType};
pub use registry::{InstalledPlugin, PluginRegistry};

use smol_str::SmolStr;

use crate::storage::StorageError;

/// Errors surfaced by the plugin runtime (spec §8.1 — hand-rolled, no
/// `thiserror`).
#[derive(Debug)]
pub enum PluginError {
    /// Filesystem I/O (`std::fs::*`) failed.
    Io(std::io::Error),
    /// Underlying [`StorageError`] from the atomic-JSON helpers (registry
    /// load/save).
    Storage(StorageError),
    /// JSON parse failure on `manifest.json`.
    Json(serde_json::Error),
    /// Manifest schema validation failed.
    ManifestInvalid(SmolStr),
    /// A plugin with the requested ID is not installed.
    NotFound(SmolStr),
    /// A plugin with the requested ID is already installed (would be raised
    /// by `install_from_zip` when the registry or install directory already
    /// contains the plugin).
    Conflict(SmolStr),
    /// Archive read/extraction validation failed.
    Archive(SmolStr),
}

impl core::fmt::Display for PluginError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "plugin io error: {e}"),
            Self::Storage(e) => write!(f, "plugin storage error: {e}"),
            Self::Json(e) => write!(f, "plugin manifest json error: {e}"),
            Self::ManifestInvalid(reason) => write!(f, "plugin manifest invalid: {reason}"),
            Self::NotFound(id) => write!(f, "plugin '{id}' is not installed"),
            Self::Conflict(id) => write!(f, "plugin '{id}' is already installed"),
            Self::Archive(reason) => write!(f, "plugin archive invalid: {reason}"),
        }
    }
}

impl core::error::Error for PluginError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Storage(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::ManifestInvalid(_) | Self::NotFound(_) | Self::Conflict(_) | Self::Archive(_) => {
                None
            }
        }
    }
}

impl From<StorageError> for PluginError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

impl From<std::io::Error> for PluginError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for PluginError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn display_renders_all_variants() {
        let v = vec![
            PluginError::ManifestInvalid(SmolStr::new_inline("bad id")),
            PluginError::NotFound(SmolStr::new_inline("x")),
            PluginError::Conflict(SmolStr::new_inline("x")),
            PluginError::Archive(SmolStr::new_inline("zip")),
        ];
        for e in v {
            let s = format!("{e}");
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn io_into_plugin_error() {
        let io = std::io::Error::other("boom");
        let e: PluginError = io.into();
        assert!(matches!(e, PluginError::Io(_)));
    }
}
