//! Plugin manifest schema and validation.
//!
//! Each `.bdplugin` ZIP must contain a `manifest.json` at its root conforming
//! to [`PluginManifest`]. The manifest is validated during installation to
//! ensure required fields are present and well-formed.

use serde::{Deserialize, Serialize};

use crate::error::BentoDeskError;

/// Parsed contents of a plugin's `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    pub author: String,
    pub description: String,
    pub min_app_version: Option<String>,
    pub icon: Option<String>,
}

/// The category of functionality a plugin provides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    Theme,
    Widget,
    Organizer,
}

impl PluginManifest {
    /// Validate all manifest fields.
    ///
    /// Returns `Ok(())` when the manifest passes all checks, or a
    /// [`BentoDeskError::PluginError`] describing the first violation found.
    pub fn validate(&self) -> Result<(), BentoDeskError> {
        // ID: reverse-domain-ish format — lowercase alphanumeric, dots, hyphens.
        // Must start with a letter, minimum 3 chars.
        if self.id.len() < 3 || self.id.len() > 128 {
            return Err(BentoDeskError::PluginError(
                "Plugin ID must be 3-128 characters".into(),
            ));
        }
        if !self.id.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Err(BentoDeskError::PluginError(
                "Plugin ID must start with a lowercase letter".into(),
            ));
        }
        if !self
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
        {
            return Err(BentoDeskError::PluginError(
                "Plugin ID must only contain lowercase letters, digits, dots, and hyphens".into(),
            ));
        }

        // Name: 1-100 chars, non-empty
        if self.name.is_empty() || self.name.len() > 100 {
            return Err(BentoDeskError::PluginError(
                "Plugin name must be 1-100 characters".into(),
            ));
        }

        // Version: basic SemVer check (MAJOR.MINOR.PATCH)
        if !is_valid_semver(&self.version) {
            return Err(BentoDeskError::PluginError(format!(
                "Invalid plugin version '{}': expected SemVer (e.g. 1.0.0)",
                self.version
            )));
        }

        // Type: v0.1 only supports Theme
        if self.plugin_type != PluginType::Theme {
            return Err(BentoDeskError::PluginError(
                "Only 'theme' plugins are supported in this version".into(),
            ));
        }

        // Author: non-empty
        if self.author.trim().is_empty() {
            return Err(BentoDeskError::PluginError(
                "Plugin author must not be empty".into(),
            ));
        }

        // Description: non-empty
        if self.description.trim().is_empty() {
            return Err(BentoDeskError::PluginError(
                "Plugin description must not be empty".into(),
            ));
        }

        Ok(())
    }
}

/// Basic SemVer validation: MAJOR.MINOR.PATCH where each part is a non-negative integer.
fn is_valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.parse::<u64>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> PluginManifest {
        PluginManifest {
            id: "com.example.mytheme".into(),
            name: "My Theme".into(),
            version: "1.0.0".into(),
            plugin_type: PluginType::Theme,
            author: "Test Author".into(),
            description: "A test theme plugin".into(),
            min_app_version: None,
            icon: None,
        }
    }

    #[test]
    fn valid_manifest_passes() {
        assert!(valid_manifest().validate().is_ok());
    }

    #[test]
    fn rejects_short_id() {
        let mut m = valid_manifest();
        m.id = "ab".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_uppercase_id() {
        let mut m = valid_manifest();
        m.id = "Com.Example".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_id_starting_with_digit() {
        let mut m = valid_manifest();
        m.id = "1plugin".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_empty_name() {
        let mut m = valid_manifest();
        m.name = "".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_invalid_semver() {
        let mut m = valid_manifest();
        m.version = "1.0".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_non_theme_type() {
        let mut m = valid_manifest();
        m.plugin_type = PluginType::Widget;
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_empty_author() {
        let mut m = valid_manifest();
        m.author = "  ".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn rejects_empty_description() {
        let mut m = valid_manifest();
        m.description = "".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn semver_valid_cases() {
        assert!(is_valid_semver("0.0.1"));
        assert!(is_valid_semver("1.2.3"));
        assert!(is_valid_semver("10.20.30"));
    }

    #[test]
    fn semver_invalid_cases() {
        assert!(!is_valid_semver("1.0"));
        assert!(!is_valid_semver("1.0.0.0"));
        assert!(!is_valid_semver("abc"));
        assert!(!is_valid_semver("1..0"));
        assert!(!is_valid_semver(""));
    }
}
