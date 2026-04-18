//! Settings schema migration dispatcher (Theme A — A2).
//!
//! The previous string-based `AppSettings.version` field was never bumped and
//! could not express migration steps reliably. Starting with 1.2 we switch to
//! a numeric `schema_version: u32` that is:
//!
//! * `0 | 1` for any legacy 1.x payload (missing or older `schema_version`);
//! * `2` for the current 1.2 shape.
//!
//! Migrations MUST be additive — never delete a field, only reinterpret it.
//! Unrecognised fields are preserved in the opaque `_legacy` bucket so a
//! rollback to 1.1 can still read its own data. Errors here are fatal but
//! recoverable: the caller is expected to
//! [`crate::config::backup::restore_backup`] and surface the issue to the UI.

use serde_json::Value;

pub const CURRENT_SCHEMA_VERSION: u32 = 2;

/// Mutate a raw settings JSON blob in-place so it matches `CURRENT_SCHEMA_VERSION`.
///
/// `value` is passed as [`serde_json::Value`] rather than `AppSettings` so we
/// can see fields the Rust type no longer knows about and stash them under
/// `_legacy` instead of silently losing them.
pub fn migrate_in_place(value: &mut Value) -> Result<MigrationReport, MigrationError> {
    let Some(obj) = value.as_object_mut() else {
        return Err(MigrationError::NotAnObject);
    };

    let from_version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or(0);

    let mut report = MigrationReport {
        from_version,
        to_version: CURRENT_SCHEMA_VERSION,
        applied_steps: Vec::new(),
    };

    if from_version > CURRENT_SCHEMA_VERSION {
        // Future schema — we cannot safely reinterpret unknown fields. Keep
        // the payload intact so a newer build can load it again later.
        tracing::warn!(
            "Settings schema_version {} is newer than supported {}. Leaving payload untouched.",
            from_version,
            CURRENT_SCHEMA_VERSION
        );
        return Ok(report);
    }

    if from_version < 2 {
        migrate_v1_to_v2(obj)?;
        report.applied_steps.push("v1_to_v2".to_string());
    }

    obj.insert(
        "schema_version".to_string(),
        Value::from(CURRENT_SCHEMA_VERSION),
    );
    Ok(report)
}

/// v1 → v2 (additive):
///   * Introduce `schema_version` numeric field.
///   * Ensure `updates` / `encryption` sub-objects exist with safe defaults
///     so serde can deserialize without tripping required-field errors on
///     mixed upgrade + downgrade scenarios.
///   * Preserve unknown fields in `_legacy` rather than dropping them.
fn migrate_v1_to_v2(obj: &mut serde_json::Map<String, Value>) -> Result<(), MigrationError> {
    // Known legacy keys that were dropped between 1.1 and 1.2. Anything not
    // in the current known-fields list is parked under `_legacy` instead of
    // discarded.
    const KNOWN_1_2_FIELDS: &[&str] = &[
        "schema_version",
        "version",
        "ghost_layer_enabled",
        "expand_delay_ms",
        "collapse_delay_ms",
        "icon_cache_size",
        "auto_group_enabled",
        "theme",
        "accent_color",
        "desktop_path",
        "watch_paths",
        "portable_mode",
        "launch_at_startup",
        "show_in_taskbar",
        "safety_profile",
        "active_theme",
        "startup_high_priority",
        "crash_restart_enabled",
        "crash_max_retries",
        "crash_window_secs",
        "safe_start_after_hibernation",
        "hibernate_resume_delay_ms",
        "updates",
        "encryption",
        "debug_overlay",
        "zone_display_mode",
        "_legacy",
    ];

    let legacy_keys: Vec<String> = obj
        .keys()
        .filter(|k| !KNOWN_1_2_FIELDS.iter().any(|known| known == k))
        .cloned()
        .collect();

    if !legacy_keys.is_empty() {
        let mut legacy_bucket = obj
            .get("_legacy")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        for key in legacy_keys {
            if let Some(v) = obj.remove(&key) {
                legacy_bucket.insert(key, v);
            }
        }
        obj.insert("_legacy".to_string(), Value::Object(legacy_bucket));
    }

    obj.entry("updates").or_insert_with(|| {
        serde_json::json!({
            "check_frequency": "Weekly",
            "auto_download": true,
            "skipped_version": null
        })
    });
    obj.entry("encryption").or_insert_with(|| {
        serde_json::json!({
            "mode": "None"
        })
    });

    Ok(())
}

/// Human-readable summary that the UI can surface if needed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MigrationReport {
    pub from_version: u32,
    pub to_version: u32,
    pub applied_steps: Vec<String>,
}

impl MigrationReport {
    pub fn is_noop(&self) -> bool {
        self.applied_steps.is_empty() && self.from_version == self.to_version
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("Settings root is not a JSON object; refusing to migrate")]
    NotAnObject,
}

impl From<MigrationError> for crate::error::BentoDeskError {
    fn from(err: MigrationError) -> Self {
        crate::error::BentoDeskError::ConfigError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fresh_settings_without_version_migrates_to_current() {
        let mut value = json!({
            "version": "1.0.0",
            "ghost_layer_enabled": true,
            "expand_delay_ms": 150
        });

        let report = migrate_in_place(&mut value).unwrap();
        assert_eq!(report.from_version, 0);
        assert_eq!(report.to_version, CURRENT_SCHEMA_VERSION);
        assert!(report.applied_steps.iter().any(|s| s == "v1_to_v2"));

        assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
        assert!(value.get("updates").is_some());
        assert!(value.get("encryption").is_some());
    }

    #[test]
    fn unknown_legacy_fields_are_parked_not_dropped() {
        let mut value = json!({
            "schema_version": 1,
            "ghost_layer_enabled": true,
            "some_old_field": 42,
            "another_old": "hello"
        });

        migrate_in_place(&mut value).unwrap();

        let legacy = value
            .get("_legacy")
            .and_then(|v| v.as_object())
            .expect("_legacy bucket missing");
        assert_eq!(legacy["some_old_field"], 42);
        assert_eq!(legacy["another_old"], "hello");
        assert!(value.get("some_old_field").is_none());
    }

    #[test]
    fn already_current_is_noop_aside_from_version_stamp() {
        let mut value = json!({
            "schema_version": CURRENT_SCHEMA_VERSION,
            "ghost_layer_enabled": true
        });

        let report = migrate_in_place(&mut value).unwrap();
        assert_eq!(report.from_version, CURRENT_SCHEMA_VERSION);
        assert!(report.is_noop());
    }

    #[test]
    fn future_version_leaves_payload_intact() {
        let mut value = json!({
            "schema_version": 99,
            "future_field": "don't touch"
        });

        let report = migrate_in_place(&mut value).unwrap();
        assert_eq!(report.from_version, 99);
        assert!(report.applied_steps.is_empty());
        assert_eq!(value["future_field"], "don't touch");
    }

    #[test]
    fn non_object_root_errors() {
        let mut value = json!([1, 2, 3]);
        let err = migrate_in_place(&mut value).unwrap_err();
        assert!(matches!(err, MigrationError::NotAnObject));
    }

    #[test]
    fn migrate_preserves_debug_overlay_and_zone_display_mode() {
        // Both fields were added in 1.2 but were missing from KNOWN_1_2_FIELDS,
        // which would have silently moved them into `_legacy` on any 1.1 payload
        // that somehow carried them forward. Guard against regression.
        let mut value = json!({
            "version": "1.1.0",
            "ghost_layer_enabled": true,
            "debug_overlay": true,
            "zone_display_mode": "always"
        });

        migrate_in_place(&mut value).unwrap();

        assert_eq!(value["debug_overlay"], true);
        assert_eq!(value["zone_display_mode"], "always");
        let legacy = value.get("_legacy").and_then(|v| v.as_object());
        if let Some(bucket) = legacy {
            assert!(!bucket.contains_key("debug_overlay"));
            assert!(!bucket.contains_key("zone_display_mode"));
        }
    }

    #[test]
    fn preserves_fields_through_v1_to_v2() {
        let mut value = json!({
            "version": "1.1.0",
            "ghost_layer_enabled": false,
            "icon_cache_size": 1234,
            "theme": "Light",
            "accent_color": "#ff0000",
            "watch_paths": ["C:/foo", "D:/bar"]
        });

        migrate_in_place(&mut value).unwrap();

        assert_eq!(value["ghost_layer_enabled"], false);
        assert_eq!(value["icon_cache_size"], 1234);
        assert_eq!(value["theme"], "Light");
        assert_eq!(value["accent_color"], "#ff0000");
        assert_eq!(value["watch_paths"][0], "C:/foo");
        assert_eq!(value["watch_paths"][1], "D:/bar");
    }
}
