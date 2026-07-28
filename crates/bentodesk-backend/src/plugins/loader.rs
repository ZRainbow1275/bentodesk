//! T-096 — Plugin install / uninstall / toggle lifecycle.
//!
//! ## v0.0.x scope (Q7 ruling 2026-05-03)
//!
//! `install_from_zip` now implements the selected-stack replacement for
//! Tauri `install_plugin(path)`: archive entries are extracted into a hidden
//! temp directory under `<state_dir>/plugins`, validated, finalized into the
//! manifest-derived install directory, and persisted in the plugin registry.
//!
//! ## What changed vs 1.x
//!
//! - **Spec §8.1**: hand-rolled [`PluginError`] (in `super`).
//! - **Spec §8 (Q1 corollary)**: `chrono::Utc::now().to_rfc3339()` →
//!   [`crate::time::now_rfc3339`]. `uuid::Uuid::new_v4()` (originally
//!   used for tmp-dir naming) remains eliminated; temp dirs are derived from
//!   process id plus `SystemTime` and created with exclusive `create_dir`.
//! - **Tauri removal**: signatures take `state_dir: &Path` directly.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use smol_str::SmolStr;
use zip::read::ZipArchive;

use super::PluginError;
use super::manifest::{PluginManifest, validate_plugin_id};
use super::registry::{InstalledPlugin, PluginRegistry};
use crate::themes::{load_theme_file, to_theme_tokens};
use crate::time;

const MAX_PLUGIN_ARCHIVE_ENTRIES: usize = 512;
const MAX_PLUGIN_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PLUGIN_EXTRACTED_BYTES: u64 = 64 * 1024 * 1024;
const ZIP_UNIX_FILE_TYPE_MASK: u32 = 0o170000;
const ZIP_UNIX_SYMLINK_TYPE: u32 = 0o120000;

/// Directory within the state dir where plugins are extracted.
fn plugins_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("plugins")
}

/// Install a plugin from a `.bdplugin` / `.zip` archive.
///
/// The archive is extracted into a temporary directory with zip-slip,
/// symlink, duplicate-entry, and size checks. The manifest is then validated,
/// type-specific payload is verified, and the final install directory plus
/// registry entry are committed together as far as filesystem semantics allow.
pub fn install_from_zip(state_dir: &Path, src: &Path) -> Result<PluginManifest, PluginError> {
    ensure_archive_extension(src)?;

    let zip_file = File::open(src).map_err(PluginError::Io)?;
    let mut archive = ZipArchive::new(zip_file).map_err(|error| {
        archive_error(format!(
            "cannot open plugin archive '{}': {error}",
            src.display()
        ))
    })?;

    if archive.len() > MAX_PLUGIN_ARCHIVE_ENTRIES {
        return Err(archive_error(format!(
            "archive contains {} entries, limit is {MAX_PLUGIN_ARCHIVE_ENTRIES}",
            archive.len()
        )));
    }

    let plugins_root = plugins_dir(state_dir);
    std::fs::create_dir_all(&plugins_root).map_err(PluginError::Io)?;
    let tmp_dir = create_install_temp_dir(&plugins_root)?;

    let install_result = (|| {
        extract_zip_safely(&mut archive, &tmp_dir)?;
        let manifest = read_and_validate_manifest(&tmp_dir.join("manifest.json"))?;
        validate_payload(&manifest, &tmp_dir)?;

        let mut registry = PluginRegistry::load(state_dir)?;
        if registry.find(&manifest.id).is_some() {
            return Err(PluginError::Conflict(SmolStr::from(manifest.id.clone())));
        }

        let final_dir = install_path_for(state_dir, &manifest.id)?;
        if final_dir.exists() {
            return Err(PluginError::Conflict(SmolStr::from(format!(
                "{} (install directory already exists)",
                manifest.id
            ))));
        }

        std::fs::rename(&tmp_dir, &final_dir).map_err(PluginError::Io)?;
        let installed = build_record(manifest.clone(), &final_dir);
        registry.plugins.push(installed);
        if let Err(error) = registry.save(state_dir) {
            let _ = std::fs::remove_dir_all(&final_dir);
            return Err(error);
        }

        tracing::info!("Plugin '{}' installed successfully", manifest.id);
        Ok(manifest)
    })();

    if install_result.is_err() && tmp_dir.exists() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    install_result
}

fn archive_error(message: impl Into<String>) -> PluginError {
    PluginError::Archive(SmolStr::from(message.into()))
}

fn ensure_archive_extension(src: &Path) -> Result<(), PluginError> {
    let extension = src
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("bdplugin" | "zip") => Ok(()),
        _ => Err(archive_error(format!(
            "unsupported plugin archive extension for '{}'; expected .bdplugin or .zip",
            src.display()
        ))),
    }
}

fn create_install_temp_dir(plugins_root: &Path) -> Result<PathBuf, PluginError> {
    let process_id = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    for attempt in 0..16_u8 {
        let candidate = plugins_root.join(format!(".tmp-install-{process_id}-{nanos}-{attempt}"));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(PluginError::Io(error)),
        }
    }
    Err(archive_error(
        "could not allocate a unique plugin install temp directory",
    ))
}

fn extract_zip_safely(archive: &mut ZipArchive<File>, dest_dir: &Path) -> Result<(), PluginError> {
    let canonical_dest = std::fs::canonicalize(dest_dir).map_err(PluginError::Io)?;
    let mut total_written = 0_u64;

    for entry_index in 0..archive.len() {
        let mut entry = archive.by_index(entry_index).map_err(|error| {
            archive_error(format!("cannot read ZIP entry {entry_index}: {error}"))
        })?;

        if let Some(mode) = entry.unix_mode() {
            if mode & ZIP_UNIX_FILE_TYPE_MASK == ZIP_UNIX_SYMLINK_TYPE {
                return Err(archive_error(format!(
                    "ZIP entry '{}' is a symlink, which is not supported",
                    entry.name()
                )));
            }
        }

        let entry_path = entry
            .enclosed_name()
            .ok_or_else(|| archive_error(format!("ZIP entry has unsafe path: {}", entry.name())))?;
        ensure_relative_archive_path(entry_path)?;
        let target = dest_dir.join(entry_path);

        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(PluginError::Io)?;
            continue;
        }

        if entry.size() > MAX_PLUGIN_ENTRY_BYTES {
            return Err(archive_error(format!(
                "ZIP entry '{}' is {} bytes, limit is {MAX_PLUGIN_ENTRY_BYTES}",
                entry.name(),
                entry.size()
            )));
        }

        if target.exists() {
            return Err(archive_error(format!(
                "ZIP entry '{}' would overwrite a previous entry",
                entry_path.display()
            )));
        }

        let parent = target.parent().unwrap_or(dest_dir);
        std::fs::create_dir_all(parent).map_err(PluginError::Io)?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(PluginError::Io)?;
        if !canonical_parent.starts_with(&canonical_dest) {
            return Err(archive_error(format!(
                "ZIP entry '{}' escapes destination directory",
                entry_path.display()
            )));
        }

        let mut output = File::create(&target).map_err(PluginError::Io)?;
        let written = copy_entry_bounded(&mut entry, &mut output, MAX_PLUGIN_ENTRY_BYTES)?;
        total_written = total_written.saturating_add(written);
        if total_written > MAX_PLUGIN_EXTRACTED_BYTES {
            return Err(archive_error(format!(
                "archive expands beyond {MAX_PLUGIN_EXTRACTED_BYTES} bytes"
            )));
        }
    }

    Ok(())
}

fn ensure_relative_archive_path(path: &Path) -> Result<(), PluginError> {
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(archive_error(format!(
                    "ZIP entry path '{}' is not relative and safe",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn copy_entry_bounded(
    input: &mut impl Read,
    output: &mut impl Write,
    max_bytes: u64,
) -> Result<u64, PluginError> {
    let mut buffer = [0_u8; 8192];
    let mut written = 0_u64;
    loop {
        let read = input.read(&mut buffer).map_err(PluginError::Io)?;
        if read == 0 {
            return Ok(written);
        }
        written = written.saturating_add(read as u64);
        if written > max_bytes {
            return Err(archive_error(format!(
                "ZIP entry expands beyond {max_bytes} bytes"
            )));
        }
        output.write_all(&buffer[..read]).map_err(PluginError::Io)?;
    }
}

fn read_and_validate_manifest(path: &Path) -> Result<PluginManifest, PluginError> {
    if !path.is_file() {
        return Err(PluginError::ManifestInvalid(SmolStr::from(
            "Plugin archive must contain a manifest.json at its root",
        )));
    }
    let content = std::fs::read_to_string(path).map_err(PluginError::Io)?;
    let manifest: PluginManifest = serde_json::from_str(&content).map_err(PluginError::Json)?;
    manifest.validate()?;
    Ok(manifest)
}

fn validate_payload(_manifest: &PluginManifest, plugin_dir: &Path) -> Result<(), PluginError> {
    let theme_json = plugin_dir.join("theme.json");
    if !theme_json.is_file() {
        return Err(PluginError::ManifestInvalid(SmolStr::from(
            "Theme plugin must contain a theme.json file",
        )));
    }
    let theme = load_theme_file(&theme_json)
        .map_err(|error| PluginError::ManifestInvalid(SmolStr::from(error.to_string())))?;
    to_theme_tokens(&theme)
        .map_err(|error| PluginError::ManifestInvalid(SmolStr::from(error.to_string())))?;
    Ok(())
}

/// Uninstall a plugin by ID.
///
/// Removes the plugin directory (if it still exists on disk) and its
/// registry entry. Idempotent for the registry side: removing a plugin
/// that was never installed is a no-op `Ok(())` after a `tracing::warn!`,
/// because the user-visible outcome is identical.
pub fn uninstall(id: &str, state_dir: &Path) -> Result<(), PluginError> {
    let mut registry = PluginRegistry::load(state_dir)?;

    if registry.find(id).is_none() {
        tracing::warn!("uninstall: plugin '{id}' is not in the registry");
        return Ok(());
    }

    // The registry is persisted user-writable JSON. Never trust its historical
    // `install_path`: validate the ID and derive the only directory this
    // lifecycle owns under the active state root.
    let install_path = install_path_for(state_dir, id)?;

    if install_path.exists() {
        std::fs::remove_dir_all(&install_path).map_err(PluginError::Io)?;
    }

    registry.remove(id);
    registry.save(state_dir)?;

    tracing::info!("Plugin '{id}' uninstalled successfully");
    Ok(())
}

/// Toggle a plugin's enabled state. Returns the updated record.
pub fn toggle_enabled(
    id: &str,
    enabled: bool,
    state_dir: &Path,
) -> Result<InstalledPlugin, PluginError> {
    let mut registry = PluginRegistry::load(state_dir)?;

    let plugin = registry
        .find_mut(id)
        .ok_or_else(|| PluginError::NotFound(SmolStr::from(id)))?;

    plugin.enabled = enabled;
    let updated = plugin.clone();
    registry.save(state_dir)?;

    tracing::info!("Plugin '{id}' enabled={enabled}");
    Ok(updated)
}

/// Build an [`InstalledPlugin`] record from a validated manifest plus the
/// final on-disk install path. Used by `install_from_zip` (when it lands
/// in `T-096b`) and by tests for the registry.
///
/// Time stamp comes from [`time::now_rfc3339`] (Q1 — no chrono).
pub fn build_record(manifest: PluginManifest, install_path: &Path) -> InstalledPlugin {
    InstalledPlugin::from_manifest(manifest, install_path, true, time::now_rfc3339())
}

/// Return the on-disk path a plugin with `id` would be installed to.
pub fn install_path_for(state_dir: &Path, id: &str) -> Result<PathBuf, PluginError> {
    validate_plugin_id(id)?;
    Ok(plugins_dir(state_dir).join(id))
}

#[cfg(test)]
mod tests {
    use super::super::manifest::PluginType;
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use zip::write::FileOptions;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn scratch_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let tid = std::thread::current().id();
        let path = std::env::temp_dir().join(format!("bentodesk-plugin-loader-{tid:?}-{n}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn sample_manifest() -> PluginManifest {
        PluginManifest {
            id: "com.example.theme".into(),
            name: "Sample".into(),
            version: "1.2.3".into(),
            plugin_type: PluginType::Theme,
            author: "Tester".into(),
            description: "Test".into(),
            min_app_version: None,
            icon: None,
        }
    }

    fn sample_theme_json(id: &str) -> String {
        format!(
            r##"{{
  "id": "{id}",
  "name": "Plugin Theme",
  "is_builtin": false,
  "colors": {{
    "accent": "#0ea5e9",
    "background": "rgba(8, 47, 73, 0.75)",
    "text": "#e0f2fe",
    "border": "rgba(14, 165, 233, 0.2)"
  }},
  "capsule": {{
    "shape": "rounded",
    "size": "medium",
    "blur_radius": 20.0
  }},
  "animation": {{
    "expand_duration_ms": 250,
    "collapse_duration_ms": 200
  }},
  "glassmorphism": {{
    "blur": 20.0,
    "opacity": 0.75,
    "saturation": 1.6
  }}
}}"##
        )
    }

    fn write_plugin_archive(path: &Path, entries: &[(&str, Vec<u8>)]) {
        let file = File::create(path).expect("create archive");
        let mut writer = zip::ZipWriter::new(file);
        let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("start file");
            writer.write_all(bytes).expect("write file");
        }
        writer.finish().expect("finish archive");
    }

    fn write_valid_plugin_archive(path: &Path) {
        let manifest = sample_manifest();
        let manifest_json = serde_json::to_string_pretty(&manifest).expect("manifest json");
        write_plugin_archive(
            path,
            &[
                ("manifest.json", manifest_json.into_bytes()),
                ("theme.json", sample_theme_json("plugin-theme").into_bytes()),
            ],
        );
    }

    #[test]
    fn install_from_zip_extracts_valid_archive_and_persists_registry() {
        let dir = scratch_dir();
        let src = dir.join("sample.bdplugin");
        write_valid_plugin_archive(&src);

        let manifest = install_from_zip(&dir, &src).expect("install");

        assert_eq!(manifest.id, "com.example.theme");
        let install_dir =
            install_path_for(&dir, "com.example.theme").expect("safe plugin install path");
        assert!(install_dir.join("manifest.json").is_file());
        assert!(install_dir.join("theme.json").is_file());

        let registry = PluginRegistry::load(&dir).expect("registry");
        let plugin = registry.find("com.example.theme").expect("registry row");
        assert!(plugin.enabled);
        assert_eq!(plugin.name, "Sample");
        assert_eq!(PathBuf::from(&plugin.install_path), install_dir);
    }

    #[test]
    fn install_from_zip_rejects_duplicate_plugin_id() {
        let dir = scratch_dir();
        let src = dir.join("sample.bdplugin");
        write_valid_plugin_archive(&src);

        install_from_zip(&dir, &src).expect("first install");

        match install_from_zip(&dir, &src) {
            Err(PluginError::Conflict(id)) => assert!(id.contains("com.example.theme")),
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn install_from_zip_rejects_zip_slip_and_cleans_temp_dir() {
        let dir = scratch_dir();
        let src = dir.join("unsafe.bdplugin");
        let manifest_json = serde_json::to_string_pretty(&sample_manifest()).expect("manifest");
        write_plugin_archive(
            &src,
            &[
                ("manifest.json", manifest_json.into_bytes()),
                ("../evil.txt", b"escape".to_vec()),
            ],
        );

        match install_from_zip(&dir, &src) {
            Err(PluginError::Archive(message)) => {
                assert!(message.contains("unsafe") || message.contains("escapes"));
            }
            other => panic!("expected Archive, got {other:?}"),
        }
        assert!(!dir.join("evil.txt").exists());
        let plugins_root = dir.join("plugins");
        if plugins_root.exists() {
            let tmp_count = std::fs::read_dir(&plugins_root)
                .expect("plugins dir")
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".tmp-install-")
                })
                .count();
            assert_eq!(tmp_count, 0);
        }
    }

    #[test]
    fn install_from_zip_rejects_missing_theme_payload() {
        let dir = scratch_dir();
        let src = dir.join("missing-theme.bdplugin");
        let manifest_json = serde_json::to_string_pretty(&sample_manifest()).expect("manifest");
        write_plugin_archive(&src, &[("manifest.json", manifest_json.into_bytes())]);

        match install_from_zip(&dir, &src) {
            Err(PluginError::ManifestInvalid(message)) => {
                assert!(message.contains("theme.json"));
            }
            other => panic!("expected ManifestInvalid, got {other:?}"),
        }
        assert!(PluginRegistry::load(&dir).unwrap().plugins.is_empty());
    }

    #[test]
    fn install_from_zip_rejects_invalid_theme_json() {
        let dir = scratch_dir();
        let src = dir.join("invalid-theme.bdplugin");
        let manifest_json = serde_json::to_string_pretty(&sample_manifest()).expect("manifest");
        write_plugin_archive(
            &src,
            &[
                ("manifest.json", manifest_json.into_bytes()),
                ("theme.json", br#"{"id":"bad"}"#.to_vec()),
            ],
        );

        match install_from_zip(&dir, &src) {
            Err(PluginError::ManifestInvalid(message)) => {
                assert!(message.contains("theme"));
            }
            other => panic!("expected ManifestInvalid, got {other:?}"),
        }
        assert!(PluginRegistry::load(&dir).unwrap().plugins.is_empty());
    }

    #[test]
    fn install_from_zip_rejects_non_plugin_extension() {
        let dir = scratch_dir();
        let src = dir.join("sample.txt");
        std::fs::write(&src, b"not zip").expect("write");

        match install_from_zip(&dir, &src) {
            Err(PluginError::Archive(message)) => assert!(message.contains("extension")),
            other => panic!("expected Archive, got {other:?}"),
        }
    }

    #[test]
    fn install_path_for_is_under_plugins_subdir() {
        let dir = scratch_dir();
        let p = install_path_for(&dir, "com.example.theme").expect("safe plugin install path");
        assert!(
            p.ends_with("plugins/com.example.theme") || p.ends_with("plugins\\com.example.theme")
        );
    }

    #[test]
    fn build_record_carries_validated_manifest_and_installed_at_is_rfc3339() {
        let dir = scratch_dir();
        let p = install_path_for(&dir, "com.example.theme").expect("safe plugin install path");
        let rec = build_record(sample_manifest(), &p);
        assert_eq!(rec.id, "com.example.theme");
        assert!(rec.enabled);
        assert!(rec.installed_at.ends_with('Z'));
        assert!(rec.installed_at.len() >= 20);
    }

    #[test]
    fn uninstall_missing_plugin_is_idempotent() {
        let dir = scratch_dir();
        uninstall("never.installed", &dir).expect("idempotent ok");
    }

    #[test]
    fn uninstall_removes_registry_entry_and_dir() {
        let dir = scratch_dir();
        let plug_dir =
            install_path_for(&dir, "com.example.theme").expect("safe plugin install path");
        std::fs::create_dir_all(&plug_dir).unwrap();
        std::fs::write(plug_dir.join("theme.json"), "{}").unwrap();

        let mut reg = PluginRegistry::default();
        reg.plugins.push(build_record(sample_manifest(), &plug_dir));
        reg.save(&dir).unwrap();

        uninstall("com.example.theme", &dir).expect("uninstall");

        assert!(!plug_dir.exists(), "plugin dir should be gone");
        let reloaded = PluginRegistry::load(&dir).unwrap();
        assert!(reloaded.plugins.is_empty());
    }

    #[test]
    fn uninstall_ignores_tampered_registry_install_path() {
        let dir = scratch_dir();
        let outside_dir = dir.join("outside-plugin-data");
        std::fs::create_dir_all(&outside_dir).expect("outside dir");
        let sentinel = outside_dir.join("keep.txt");
        std::fs::write(&sentinel, "keep").expect("outside sentinel");

        let mut record = build_record(sample_manifest(), &outside_dir);
        record.install_path = outside_dir.to_string_lossy().into_owned();
        let mut registry = PluginRegistry::default();
        registry.plugins.push(record);
        registry.save(&dir).expect("save tampered registry");

        uninstall("com.example.theme", &dir).expect("safe uninstall");

        assert!(
            sentinel.is_file(),
            "uninstall must never follow a persisted path outside plugins/<id>"
        );
        assert!(
            PluginRegistry::load(&dir)
                .expect("reload")
                .find("com.example.theme")
                .is_none()
        );
    }

    #[test]
    fn install_path_for_rejects_unsafe_id() {
        assert!(matches!(
            install_path_for(Path::new("state"), "../outside"),
            Err(PluginError::ManifestInvalid(_))
        ));
    }

    #[test]
    fn toggle_enabled_flips_persisted_flag() {
        let dir = scratch_dir();
        let plug_dir =
            install_path_for(&dir, "com.example.theme").expect("safe plugin install path");
        std::fs::create_dir_all(&plug_dir).unwrap();

        let mut reg = PluginRegistry::default();
        reg.plugins.push(build_record(sample_manifest(), &plug_dir));
        reg.save(&dir).unwrap();

        let after = toggle_enabled("com.example.theme", false, &dir).expect("toggle");
        assert!(!after.enabled);

        let reloaded = PluginRegistry::load(&dir).unwrap();
        assert!(!reloaded.find("com.example.theme").unwrap().enabled);
    }

    #[test]
    fn toggle_enabled_unknown_id_returns_not_found() {
        let dir = scratch_dir();
        match toggle_enabled("never.installed", true, &dir) {
            Err(PluginError::NotFound(id)) => assert_eq!(id, "never.installed"),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }
}
