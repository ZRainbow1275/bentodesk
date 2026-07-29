//! T-096 — Plugin registry persisted as `<state_dir>/plugins/registry.json`.
//!
//! Uses the same atomic-write + backup-recovery system as layout and settings
//! (see [`crate::storage`]).
//!
//! ## What changed vs 1.x
//!
//! - **Spec §8.1**: hand-rolled [`PluginError`] (in `super`) replaces
//!   `BentoDeskError`. Storage failures wrap `StorageError` via
//!   [`PluginError::Storage`].
//! - **Tauri removal**: all paths take `state_dir: &Path` instead of
//!   `app_data: &Path` resolved from `AppHandle::path()`.
//! - **Q1**: timestamps in [`InstalledPlugin::installed_at`] come from
//!   [`crate::time::now_rfc3339`] in [`super::loader`], not chrono.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::PluginError;
use super::manifest::{PluginManifest, PluginType};
use crate::{storage, time};

/// On-disk registry of all installed plugins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegistry {
    pub version: String,
    pub plugins: Vec<InstalledPlugin>,
}

/// A single installed plugin's metadata and state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub plugin_type: PluginType,
    pub author: String,
    pub description: String,
    pub enabled: bool,
    pub installed_at: String,
    pub install_path: String,
}

impl InstalledPlugin {
    /// Build a registry row from a validated manifest and install directory.
    pub fn from_manifest(
        manifest: PluginManifest,
        install_path: &Path,
        enabled: bool,
        installed_at: String,
    ) -> Self {
        Self {
            id: manifest.id,
            name: manifest.name,
            version: manifest.version,
            plugin_type: manifest.plugin_type,
            author: manifest.author,
            description: manifest.description,
            enabled,
            installed_at,
            install_path: install_path.to_string_lossy().into_owned(),
        }
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self {
            version: "1.0.0".into(),
            plugins: Vec::new(),
        }
    }
}

impl PluginRegistry {
    /// Path to the registry JSON file within the state directory.
    pub fn registry_path(state_dir: &Path) -> PathBuf {
        state_dir.join("plugins").join("registry.json")
    }

    /// Load the plugin registry from disk, recovering from backup if needed.
    ///
    /// Returns `Ok(default)` when no registry file exists yet, and silently
    /// downgrades a corrupt-and-unrecoverable file to the default (with a
    /// `tracing::warn!`) so the app still boots.
    pub fn load(state_dir: &Path) -> Result<Self, PluginError> {
        let path = Self::registry_path(state_dir);
        let mut registry = match storage::read_json_with_recovery::<Self>(&path, "Plugin registry")
        {
            Ok(Some(registry)) => registry,
            Ok(None) => Self::default(),
            Err(e) => {
                tracing::warn!("Failed to load plugin registry, using default: {e}");
                Self::default()
            }
        };
        registry.discover_preextracted(state_dir);
        Ok(registry)
    }

    /// Atomically persist the registry to disk.
    pub fn save(&self, state_dir: &Path) -> Result<(), PluginError> {
        let path = Self::registry_path(state_dir);
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).map_err(PluginError::Io)?;
        }
        storage::write_json_atomic(&path, self).map_err(PluginError::Storage)
    }

    /// Find a plugin by ID.
    pub fn find(&self, id: &str) -> Option<&InstalledPlugin> {
        self.plugins.iter().find(|p| p.id == id)
    }

    /// Find a mutable reference to a plugin by ID.
    pub fn find_mut(&mut self, id: &str) -> Option<&mut InstalledPlugin> {
        self.plugins.iter_mut().find(|p| p.id == id)
    }

    /// Remove a plugin by ID. Returns true if a plugin was removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.plugins.len();
        self.plugins.retain(|p| p.id != id);
        self.plugins.len() < before
    }

    fn discover_preextracted(&mut self, state_dir: &Path) {
        let plugins_dir = state_dir.join("plugins");
        let entries = match std::fs::read_dir(&plugins_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                tracing::warn!(
                    "plugin registry: cannot scan pre-extracted plugins at {}: {e}",
                    plugins_dir.display()
                );
                return;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    tracing::warn!("plugin registry: pre-extracted entry read failed: {e}");
                    continue;
                }
            };
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".tmp-"))
            {
                continue;
            }
            match read_preextracted_manifest(&path) {
                Ok(manifest) => {
                    if self.find(&manifest.id).is_some() {
                        continue;
                    }
                    if path.file_name().and_then(|name| name.to_str()) != Some(manifest.id.as_str())
                    {
                        tracing::warn!(
                            "plugin registry: skipping {} because manifest id '{}' does not match directory name",
                            path.display(),
                            manifest.id
                        );
                        continue;
                    }
                    if manifest.plugin_type == PluginType::Theme
                        && !path.join("theme.json").is_file()
                    {
                        tracing::warn!(
                            "plugin registry: skipping theme plugin '{}' because theme.json is missing",
                            manifest.id
                        );
                        continue;
                    }
                    self.plugins.push(InstalledPlugin::from_manifest(
                        manifest,
                        &path,
                        true,
                        time::now_rfc3339(),
                    ));
                }
                Err(error) => {
                    tracing::warn!(
                        "plugin registry: skipping pre-extracted plugin at {}: {error}",
                        path.display()
                    );
                }
            }
        }
    }
}

fn read_preextracted_manifest(plugin_dir: &Path) -> Result<PluginManifest, PluginError> {
    let manifest_path = plugin_dir.join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path).map_err(PluginError::Io)?;
    let manifest: PluginManifest = serde_json::from_str(&content).map_err(PluginError::Json)?;
    manifest.validate()?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn scratch_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let tid = std::thread::current().id();
        let path = std::env::temp_dir().join(format!("bentodesk-plugin-reg-{tid:?}-{n}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn sample_plugin() -> InstalledPlugin {
        InstalledPlugin {
            id: "com.test.plugin".into(),
            name: "Test Plugin".into(),
            version: "1.0.0".into(),
            plugin_type: PluginType::Theme,
            author: "Tester".into(),
            description: "A test plugin".into(),
            enabled: true,
            installed_at: "2026-01-01T00:00:00.000Z".into(),
            install_path: "/tmp/plugins/com.test.plugin".into(),
        }
    }

    fn sample_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_owned(),
            name: "Sideloaded Theme".into(),
            version: "1.0.0".into(),
            plugin_type: PluginType::Theme,
            author: "Tester".into(),
            description: "A pre-extracted theme plugin".into(),
            min_app_version: None,
            icon: None,
        }
    }

    fn write_preextracted_theme_plugin(state_dir: &Path, id: &str) -> PathBuf {
        let plugin_dir = state_dir.join("plugins").join(id);
        std::fs::create_dir_all(&plugin_dir).expect("plugin dir");
        let manifest = sample_manifest(id);
        let manifest_json = serde_json::to_string_pretty(&manifest).expect("manifest json");
        std::fs::write(plugin_dir.join("manifest.json"), manifest_json).expect("manifest write");
        std::fs::write(plugin_dir.join("theme.json"), "{}").expect("theme marker");
        plugin_dir
    }

    #[test]
    fn default_registry_is_empty() {
        let reg = PluginRegistry::default();
        assert_eq!(reg.version, "1.0.0");
        assert!(reg.plugins.is_empty());
    }

    #[test]
    fn find_returns_plugin_by_id() {
        let mut reg = PluginRegistry::default();
        reg.plugins.push(sample_plugin());
        assert!(reg.find("com.test.plugin").is_some());
        assert!(reg.find("nonexistent").is_none());
    }

    #[test]
    fn find_mut_returns_mutable_plugin() {
        let mut reg = PluginRegistry::default();
        reg.plugins.push(sample_plugin());
        let p = reg.find_mut("com.test.plugin").expect("present");
        p.enabled = false;
        assert!(!reg.find("com.test.plugin").unwrap().enabled);
    }

    #[test]
    fn remove_deletes_plugin_by_id() {
        let mut reg = PluginRegistry::default();
        reg.plugins.push(sample_plugin());
        assert!(reg.remove("com.test.plugin"));
        assert!(reg.plugins.is_empty());
    }

    #[test]
    fn remove_returns_false_for_missing_id() {
        let mut reg = PluginRegistry::default();
        assert!(!reg.remove("nonexistent"));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = scratch_dir();
        let mut reg = PluginRegistry::default();
        reg.plugins.push(sample_plugin());
        reg.save(&dir).expect("save");

        let loaded = PluginRegistry::load(&dir).expect("load");
        assert_eq!(loaded.plugins.len(), 1);
        assert_eq!(loaded.plugins[0].id, "com.test.plugin");
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let dir = scratch_dir();
        let reg = PluginRegistry::load(&dir).expect("load");
        assert!(reg.plugins.is_empty());
    }

    #[test]
    fn load_discovers_preextracted_theme_plugin_with_manifest() {
        let dir = scratch_dir();
        let plugin_dir = write_preextracted_theme_plugin(&dir, "com.test.sideloaded");

        let reg = PluginRegistry::load(&dir).expect("load");
        assert_eq!(reg.plugins.len(), 1);
        assert_eq!(reg.plugins[0].id, "com.test.sideloaded");
        assert!(reg.plugins[0].enabled);
        assert_eq!(reg.plugins[0].plugin_type, PluginType::Theme);
        assert_eq!(PathBuf::from(&reg.plugins[0].install_path), plugin_dir);
    }

    #[test]
    fn saved_registry_state_wins_over_preextracted_manifest_defaults() {
        let dir = scratch_dir();
        let plugin_dir = write_preextracted_theme_plugin(&dir, "com.test.disabled");
        let mut registry = PluginRegistry::default();
        registry.plugins.push(InstalledPlugin::from_manifest(
            sample_manifest("com.test.disabled"),
            &plugin_dir,
            false,
            "2026-01-01T00:00:00.000Z".into(),
        ));
        registry.save(&dir).expect("save disabled");

        let reg = PluginRegistry::load(&dir).expect("load");
        assert_eq!(reg.plugins.len(), 1);
        assert_eq!(reg.plugins[0].id, "com.test.disabled");
        assert!(
            !reg.plugins[0].enabled,
            "pre-extracted discovery must not re-enable a persisted disabled plugin"
        );
    }

    #[test]
    fn registry_path_is_under_plugins_subdir() {
        let dir = scratch_dir();
        let p = PluginRegistry::registry_path(&dir);
        assert!(p.ends_with("plugins/registry.json") || p.ends_with("plugins\\registry.json"));
    }
}
