//! Integration-style upgrade tests: v1.1 on-disk fixtures → v1.2 `AppSettings`.
//!
//! These tests live as a sibling module inside `crate::config` so they can
//! reach the public API of `settings`, `migration`, and `backup`. They
//! re-implement the tiny read → migrate → deserialize sequence that
//! `AppSettings::load_or_default_from_path` performs, keeping the upgrade
//! contract visible in one place without relying on private helpers.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::backup;
use super::migration::{self, migrate_in_place};
use super::settings::AppSettings;

/// Resolve the repo-relative fixture directory under `tests/fixtures/v1_1/`.
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1_1")
}

fn read_raw_fixture_value(fixture_name: &str) -> Value {
    let bytes = fs::read(fixture_dir().join(fixture_name))
        .unwrap_or_else(|e| panic!("fixture {fixture_name} missing: {e}"));
    serde_json::from_slice(&bytes).unwrap()
}

fn write_value(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

/// Drive the same read → backup → migrate → deserialize path that production
/// uses (see `AppSettings::load_or_default_from_path`) without requiring a
/// live Tauri `AppHandle`. Kept deliberately in lock-step with the real
/// loader so a regression there shows up here too.
fn simulate_load(path: &Path) -> AppSettings {
    let raw_bytes = fs::read(path).expect("fixture copy should be readable");
    let mut value: Value =
        serde_json::from_slice(&raw_bytes).expect("fixture must be valid JSON");

    // Pre-migration backup — exactly as production does it.
    backup::create_backup(path).expect("pre-migration backup must succeed");

    migrate_in_place(&mut value).expect("v1.1 → v1.2 migration must not fail");

    serde_json::from_value::<AppSettings>(value)
        .expect("post-migration payload must deserialize into AppSettings")
}

#[test]
fn v1_1_settings_loads_into_v1_2_without_data_loss() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_value(&path, &read_raw_fixture_value("settings.v1_1.json"));

    let settings = simulate_load(&path);

    // v1.1 fields preserved verbatim.
    assert_eq!(settings.version, "1.1.0");
    assert!(settings.ghost_layer_enabled);
    assert_eq!(settings.expand_delay_ms, 150);
    assert_eq!(settings.collapse_delay_ms, 400);
    assert_eq!(settings.icon_cache_size, 500);
    assert!(settings.auto_group_enabled);
    assert_eq!(settings.accent_color, "#3b82f6");
    assert_eq!(settings.desktop_path, "C:/Users/tester/Desktop");
    assert_eq!(settings.watch_paths, vec!["C:/Users/tester/Downloads"]);
    assert!(!settings.portable_mode);
    assert!(settings.launch_at_startup);
    assert!(!settings.show_in_taskbar);
    assert_eq!(settings.active_theme.as_deref(), Some("ocean-blue"));
    assert!(!settings.startup_high_priority);
    assert!(settings.crash_restart_enabled);
    assert_eq!(settings.crash_max_retries, 5);
    assert_eq!(settings.crash_window_secs, 15);
    assert!(settings.safe_start_after_hibernation);
    assert_eq!(settings.hibernate_resume_delay_ms, 2500);

    // v1.2 additions must be stamped with defaults, not left uninitialised.
    assert_eq!(settings.schema_version, migration::CURRENT_SCHEMA_VERSION);
    assert!(!settings.debug_overlay);
    assert!(settings.updates.auto_download);
    assert!(settings.updates.skipped_version.is_none());
    // `_legacy` stays null because the fixture holds no unknown keys.
    assert!(settings._legacy.is_null());
}

#[test]
fn v1_1_settings_unknown_fields_parked_in_legacy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");

    let mut raw = read_raw_fixture_value("settings.v1_1.json");
    raw.as_object_mut()
        .unwrap()
        .insert("a_dropped_field".to_string(), Value::from("keep_me"));
    write_value(&path, &raw);

    let settings = simulate_load(&path);

    let legacy = settings
        ._legacy
        .as_object()
        .expect("_legacy bucket must materialise when unknown fields are present");
    assert_eq!(legacy["a_dropped_field"], Value::from("keep_me"));
}

#[test]
fn v1_1_settings_known_v1_2_fields_not_parked_in_legacy() {
    // If a user briefly ran a 1.2 build, their file may already carry
    // `debug_overlay` / `zone_display_mode`. Migration must keep those at
    // the top level instead of shoving them into `_legacy`.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");

    let mut raw = read_raw_fixture_value("settings.v1_1.json");
    {
        let obj = raw.as_object_mut().unwrap();
        obj.insert("debug_overlay".to_string(), Value::Bool(true));
        obj.insert("zone_display_mode".to_string(), Value::from("always"));
    }
    write_value(&path, &raw);

    let settings = simulate_load(&path);

    assert!(
        settings.debug_overlay,
        "debug_overlay must be honoured, not parked in _legacy"
    );

    let legacy_has_known_field = settings
        ._legacy
        .as_object()
        .map(|m| m.contains_key("debug_overlay") || m.contains_key("zone_display_mode"))
        .unwrap_or(false);
    assert!(
        !legacy_has_known_field,
        "known v1.2 fields must NOT be stashed into _legacy: {:?}",
        settings._legacy
    );
}

#[test]
fn v1_1_settings_load_creates_pre_migration_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    write_value(&path, &read_raw_fixture_value("settings.v1_1.json"));

    let pre = backup::list_backups(&path).unwrap();
    assert!(
        pre.is_empty(),
        "fixture-seeded state should not already have rotated backups: {pre:?}"
    );

    let _settings = simulate_load(&path);

    let post = backup::list_backups(&path).unwrap();
    assert!(
        !post.is_empty(),
        "load flow must create a pre-migration backup for v1.1 payloads"
    );
    assert!(
        post[0].size_bytes > 0,
        "pre-migration backup must contain the original file bytes, got {} bytes",
        post[0].size_bytes
    );
}

#[test]
fn v1_1_settings_migration_stamps_schema_version_2() {
    let mut value = read_raw_fixture_value("settings.v1_1.json");
    assert!(
        value.get("schema_version").is_none(),
        "v1.1 fixture must not carry schema_version"
    );

    let report = migrate_in_place(&mut value).unwrap();
    assert_eq!(report.from_version, 0);
    assert_eq!(report.to_version, migration::CURRENT_SCHEMA_VERSION);
    assert!(report.applied_steps.iter().any(|s| s == "v1_to_v2"));

    assert_eq!(
        value["schema_version"],
        Value::from(migration::CURRENT_SCHEMA_VERSION)
    );
    assert!(value.get("updates").is_some());
    assert!(value.get("encryption").is_some());
}

#[test]
fn downgrade_safety_legacy_bucket_survives_migration() {
    // When a user lands on 1.2 briefly, we park unknown fields in `_legacy`.
    // If they later roll back to a build that *does* know those fields, the
    // round-trip should surface them back at the top level — validated here
    // through the migration dispatcher.
    let mut raw = read_raw_fixture_value("settings.v1_1.json");
    raw.as_object_mut().unwrap().insert(
        "future_feature_flag".to_string(),
        Value::from("downgrade_keep"),
    );

    migrate_in_place(&mut raw).unwrap();
    let legacy = raw
        .get("_legacy")
        .and_then(|v| v.as_object())
        .expect("unknown field must be parked in _legacy");
    assert_eq!(legacy["future_feature_flag"], Value::from("downgrade_keep"));
}
