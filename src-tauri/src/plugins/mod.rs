//! Plugin system — installation, management, and lifecycle for `.bdplugin` packages.
//!
//! Plugins are ZIP archives containing a `manifest.json` and type-specific assets.
//! Currently only `Theme` plugins are supported. Installed plugins are tracked in
//! `{app_data}/plugins/registry.json`.

pub mod loader;
pub mod manifest;
pub mod registry;

#[allow(unused_imports)]
pub use manifest::PluginManifest;
pub use manifest::PluginType;
pub use registry::{InstalledPlugin, PluginRegistry};
