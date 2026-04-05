//! Plugin registry — tracks installed plugins and their state.
//!
//! The registry is persisted as `{app_data}/plugins/registry.json` using the
//! same atomic write / backup recovery system as layout and settings.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::manifest::PluginType;
use crate::error::BentoDeskError;
use crate::storage;

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

impl Default for PluginRegistry {
    fn default() -> Self {
        Self {
            version: "1.0.0".into(),
            plugins: Vec::new(),
        }
    }
}

impl PluginRegistry {
    /// Path to the registry JSON file within the app data directory.
    fn registry_path(app_data: &Path) -> std::path::PathBuf {
        app_data.join("plugins").join("registry.json")
    }

    /// Load the plugin registry from disk, recovering from backup if needed.
    ///
    /// Returns `Ok(default)` when no registry file exists yet.
    pub fn load(app_data: &Path) -> Result<Self, BentoDeskError> {
        let path = Self::registry_path(app_data);
        match storage::read_json_with_recovery::<Self>(&path, "Plugin registry") {
            Ok(Some(registry)) => Ok(registry),
            Ok(None) => Ok(Self::default()),
            Err(e) => {
                tracing::warn!("Failed to load plugin registry, using default: {e}");
                Ok(Self::default())
            }
        }
    }

    /// Atomically persist the registry to disk.
    pub fn save(&self, app_data: &Path) -> Result<(), BentoDeskError> {
        let path = Self::registry_path(app_data);
        storage::write_json_atomic(&path, self)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plugin() -> InstalledPlugin {
        InstalledPlugin {
            id: "com.test.plugin".into(),
            name: "Test Plugin".into(),
            version: "1.0.0".into(),
            plugin_type: PluginType::Theme,
            author: "Tester".into(),
            description: "A test plugin".into(),
            enabled: true,
            installed_at: "2026-01-01T00:00:00Z".into(),
            install_path: "/tmp/plugins/com.test.plugin".into(),
        }
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
        let dir = tempfile::tempdir().unwrap();
        let mut reg = PluginRegistry::default();
        reg.plugins.push(sample_plugin());
        reg.save(dir.path()).unwrap();

        let loaded = PluginRegistry::load(dir.path()).unwrap();
        assert_eq!(loaded.plugins.len(), 1);
        assert_eq!(loaded.plugins[0].id, "com.test.plugin");
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let reg = PluginRegistry::load(dir.path()).unwrap();
        assert!(reg.plugins.is_empty());
    }
}
