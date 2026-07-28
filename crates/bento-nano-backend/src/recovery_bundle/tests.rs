use super::*;

fn scratch_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "bento-nano-recovery-bundle-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

#[test]
fn missing_bundle_returns_none() {
    let dir = scratch_dir("missing");
    let loaded = load_bundle(&dir).expect("load missing bundle");
    assert!(loaded.is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bundle_roundtrip_preserves_validated_zones_payload() {
    let dir = scratch_dir("roundtrip");
    let zones_path = dir.join("zones.bin");
    let zones_bin = b"BNTZ-test-payload";

    let summary = refresh_zones_bundle(&dir, &zones_path, zones_bin, 3).expect("capture bundle");
    assert_eq!(summary.zone_count, 3);
    assert_eq!(summary.zones_len_bytes, zones_bin.len() as u64);
    assert!(summary.path.exists(), "bundle json must exist");

    let recovered = recover_zones_payload(&dir)
        .expect("recover bundle")
        .expect("bundle present");
    assert_eq!(recovered.zones_bin, zones_bin);
    assert!(recovered.vault.is_none());
    assert_eq!(recovered.summary.zone_count, 3);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bundle_roundtrip_preserves_validated_zones_and_vault_payload() {
    let dir = scratch_dir("roundtrip-vault");
    let zones_path = dir.join("zones.bin");
    let vault_path = dir.join("vault.bin");
    let zones_bin = b"BNTZ-test-payload";
    let vault_bin = br#"{"version":1,"mode":"None","payload":"settings"}"#;

    let summary = refresh_bundle(
        &dir,
        &zones_path,
        zones_bin,
        2,
        Some((&vault_path, vault_bin)),
        &[],
        None,
    )
    .expect("capture bundle");
    assert_eq!(summary.zone_count, 2);
    assert!(summary.vault_included);
    assert_eq!(summary.vault_len_bytes, Some(vault_bin.len() as u64));

    let recovered = recover_zones_payload(&dir)
        .expect("recover bundle")
        .expect("bundle present");
    assert_eq!(recovered.zones_bin, zones_bin);
    let recovered_vault = recovered.vault.expect("vault payload");
    assert_eq!(recovered_vault.vault_bin, vault_bin);
    let expected_vault_path = vault_path.display().to_string();
    assert_eq!(
        recovered_vault.vault_path.as_deref(),
        Some(expected_vault_path.as_str())
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bundle_roundtrip_preserves_selected_stack_user_data_sidecars() {
    let dir = scratch_dir("roundtrip-user-data");
    let zones_path = dir.join("zones.bin");
    let timeline_dir = dir.join("timeline");
    let snapshot_dir = dir.join("snapshots");
    std::fs::create_dir_all(&timeline_dir).expect("timeline dir");
    std::fs::create_dir_all(&snapshot_dir).expect("snapshot dir");
    std::fs::write(dir.join("rules.json"), br#"[{"id":"rule-1"}]"#).expect("rules");
    std::fs::write(timeline_dir.join("checkpoint-1.json"), br#"{"id":"cp-1"}"#).expect("timeline");
    std::fs::write(snapshot_dir.join("snap-1.json"), br#"{"id":"snap-1"}"#).expect("snapshot");

    let user_data_files = collect_user_data_files(&dir).expect("collect user data");
    assert_eq!(user_data_files.len(), 3);

    let summary = refresh_bundle_with_user_data(
        &dir,
        &zones_path,
        b"zones",
        1,
        RecoveryBundleSidecars {
            user_data_files: &user_data_files,
            ..RecoveryBundleSidecars::default()
        },
    )
    .expect("capture bundle with user data");
    assert_eq!(summary.user_data_file_count, 3);
    assert!(summary.user_data_len_bytes > 0);

    std::fs::remove_file(dir.join("rules.json")).expect("remove rules");
    std::fs::remove_file(timeline_dir.join("checkpoint-1.json")).expect("remove timeline");
    std::fs::remove_file(snapshot_dir.join("snap-1.json")).expect("remove snapshot");

    let payload = recover_zones_payload(&dir)
        .expect("recover bundle")
        .expect("bundle present");
    assert_eq!(payload.user_data_files.len(), 3);
    let report =
        restore_user_data_files(&dir, &payload.user_data_files).expect("restore user data");
    assert_eq!(report.restored_files, 3);
    assert_eq!(
        std::fs::read(dir.join("rules.json")).expect("restored rules"),
        br#"[{"id":"rule-1"}]"#
    );
    assert_eq!(
        std::fs::read(timeline_dir.join("checkpoint-1.json")).expect("restored timeline"),
        br#"{"id":"cp-1"}"#
    );
    assert_eq!(
        std::fs::read(snapshot_dir.join("snap-1.json")).expect("restored snapshot"),
        br#"{"id":"snap-1"}"#
    );

    let diagnostics = diagnostics_report(&dir)
        .expect("diagnostics")
        .expect("report present");
    assert_eq!(diagnostics.user_data_files.len(), 3);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn user_data_restore_rejects_path_traversal() {
    let dir = scratch_dir("user-data-traversal");
    let payload = RecoveredUserDataFile {
        relative_path: SmolStr::new_static("../escape.json"),
        bytes: b"bad".to_vec(),
        checksum: checksum_hex(b"bad"),
    };
    let err = restore_user_data_files(&dir, &[payload]).expect_err("unsafe path rejected");
    assert!(matches!(
        err,
        RecoveryBundleError::InvalidUserDataPath { .. }
    ));
    assert!(!dir.join("..").join("escape.json").exists());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn checksum_mismatch_rejects_payload_before_shell_restore() {
    let dir = scratch_dir("tamper");
    let zones_path = dir.join("zones.bin");
    refresh_zones_bundle(&dir, &zones_path, b"good-payload", 1).expect("capture bundle");

    let mut bundle = load_bundle(&dir)
        .expect("load bundle")
        .expect("bundle exists");
    bundle.zones_bin_b64 = base64_encode(b"tampered-payload");
    write_bundle(&dir, &bundle).expect("write tampered bundle");

    let err = recover_zones_payload(&dir).expect_err("tamper must fail");
    assert!(
        matches!(err, RecoveryBundleError::LengthMismatch { .. })
            || matches!(err, RecoveryBundleError::ChecksumMismatch { .. })
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn incomplete_vault_payload_rejects_before_shell_restore() {
    let dir = scratch_dir("incomplete-vault");
    let zones_path = dir.join("zones.bin");
    refresh_zones_bundle(&dir, &zones_path, b"good-payload", 1).expect("capture bundle");

    let mut bundle = load_bundle(&dir)
        .expect("load bundle")
        .expect("bundle exists");
    bundle.vault_bin_b64 = Some(base64_encode(b"vault"));
    write_bundle(&dir, &bundle).expect("write incomplete bundle");

    let err = recover_zones_payload(&dir).expect_err("incomplete vault must fail");
    assert!(matches!(err, RecoveryBundleError::IncompleteVaultPayload));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bundle_roundtrip_preserves_safety_manifest_snapshots() {
    let dir = scratch_dir("manifest");
    let zones_path = dir.join("zones.bin");
    let desktop_path = dir.join("Desktop");
    let manifest = SafetyManifest {
        schema_version: crate::stealth::MANIFEST_SCHEMA_VERSION.to_string(),
        entries: vec![crate::stealth::ManifestEntry {
            original_path: desktop_path.join("doc.txt").display().to_string(),
            hidden_path: desktop_path
                .join(".bentodesk")
                .join("1")
                .join("doc.txt")
                .display()
                .to_string(),
            zone_id: "1".to_string(),
            file_size_bytes: 42,
            hidden_at: "2026-05-08T00:00:00Z".to_string(),
            display_name: "doc.txt".to_string(),
            icon_x: Some(10),
            icon_y: Some(20),
            file_type: "File".to_string(),
        }],
        zones: Vec::new(),
        screen_width: 1920,
        screen_height: 1080,
        last_updated: "2026-05-08T00:00:00Z".to_string(),
    };
    let snapshot = RecoverySafetyManifest {
        desktop_path: desktop_path.display().to_string(),
        manifest,
    };

    let summary = refresh_bundle(&dir, &zones_path, b"zones", 1, None, &[snapshot], None)
        .expect("capture manifest bundle");
    assert_eq!(summary.safety_manifest_count, 1);

    let recovered = recover_zones_payload(&dir)
        .expect("recover bundle")
        .expect("bundle present");
    assert_eq!(recovered.safety_manifests.len(), 1);
    assert_eq!(
        recovered.safety_manifests[0].manifest.entries[0].display_name,
        "doc.txt"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bundle_roundtrip_preserves_icon_backup_sidecar() {
    let dir = scratch_dir("icon-backup");
    let zones_path = dir.join("zones.bin");
    let icon_backup = SavedIconLayout {
        icons: vec![crate::icon_positions::IconPosition {
            name: "doc.txt".to_string(),
            x: 10,
            y: 20,
        }],
        saved_at: "2026-05-08T00:00:00Z".to_string(),
        resolution: crate::icon_positions::Resolution {
            width: 1920,
            height: 1080,
        },
        dpi: 1.0,
    };

    let summary = refresh_bundle(&dir, &zones_path, b"zones", 1, None, &[], Some(icon_backup))
        .expect("capture icon backup bundle");
    assert!(summary.icon_backup_included);

    let recovered = recover_zones_payload(&dir)
        .expect("recover bundle")
        .expect("bundle present");
    let recovered_icon_backup = recovered.icon_backup.expect("icon backup");
    assert_eq!(recovered_icon_backup.icons.len(), 1);
    assert_eq!(recovered_icon_backup.icons[0].name, "doc.txt");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn diagnostics_export_preserves_validated_bundle_metadata() {
    let dir = scratch_dir("diagnostics");
    let zones_path = dir.join("zones.bin");
    let vault_path = dir.join("vault.bin");
    let desktop_path = dir.join("Desktop");
    let zones_bin = b"BNTZ-diagnostics-payload";
    let vault_bin = br#"{"version":1,"mode":"None","payload":"settings"}"#;
    let manifest = SafetyManifest {
        schema_version: crate::stealth::MANIFEST_SCHEMA_VERSION.to_string(),
        entries: vec![crate::stealth::ManifestEntry {
            original_path: desktop_path.join("doc.txt").display().to_string(),
            hidden_path: desktop_path
                .join(".bentodesk")
                .join("1")
                .join("doc.txt")
                .display()
                .to_string(),
            zone_id: "1".to_string(),
            file_size_bytes: 42,
            hidden_at: "2026-05-08T00:00:00Z".to_string(),
            display_name: "doc.txt".to_string(),
            icon_x: Some(10),
            icon_y: Some(20),
            file_type: "File".to_string(),
        }],
        zones: Vec::new(),
        screen_width: 1920,
        screen_height: 1080,
        last_updated: "2026-05-08T00:00:00Z".to_string(),
    };
    let snapshot = RecoverySafetyManifest {
        desktop_path: desktop_path.display().to_string(),
        manifest,
    };
    let icon_backup = SavedIconLayout {
        icons: vec![crate::icon_positions::IconPosition {
            name: "doc.txt".to_string(),
            x: 10,
            y: 20,
        }],
        saved_at: "2026-05-08T00:00:00Z".to_string(),
        resolution: crate::icon_positions::Resolution {
            width: 1920,
            height: 1080,
        },
        dpi: 1.0,
    };

    refresh_bundle(
        &dir,
        &zones_path,
        zones_bin,
        4,
        Some((&vault_path, vault_bin)),
        &[snapshot],
        Some(icon_backup),
    )
    .expect("capture diagnostics bundle");

    let report = export_diagnostics_report(&dir)
        .expect("export diagnostics")
        .expect("report exists");
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.zones.zone_count, 4);
    assert_eq!(report.zones.len_bytes, zones_bin.len() as u64);
    assert_eq!(report.zones.decoded_len_bytes, zones_bin.len() as u64);
    assert!(report.vault.included);
    assert_eq!(report.vault.decoded_len_bytes, Some(vault_bin.len() as u64));
    assert_eq!(report.safety_manifests.len(), 1);
    assert_eq!(report.safety_manifests[0].entry_count, 1);
    assert!(report.icon_backup.included);
    assert_eq!(report.icon_backup.icon_count, 1);
    assert!(
        diagnostics_path(&dir).exists(),
        "diagnostics json must exist"
    );

    let raw = std::fs::read(diagnostics_path(&dir)).expect("read diagnostics");
    let persisted: RecoveryDiagnosticsReport =
        serde_json::from_slice(&raw).expect("parse diagnostics");
    assert_eq!(persisted.zones.checksum, report.zones.checksum);
    assert_eq!(
        persisted.bundle_path,
        bundle_path(&dir).display().to_string()
    );

    let _ = std::fs::remove_dir_all(dir);
}
