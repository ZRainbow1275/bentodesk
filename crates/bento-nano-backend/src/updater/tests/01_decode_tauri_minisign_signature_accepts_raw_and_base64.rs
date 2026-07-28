
#[test]
fn decode_tauri_minisign_signature_accepts_raw_and_base64() {
    assert_eq!(
        decode_tauri_minisign_signature(TEST_MINISIGN_SIGNATURE).expect("raw signature"),
        TEST_MINISIGN_SIGNATURE
    );
    assert_eq!(
        decode_tauri_minisign_signature(TEST_MINISIGN_SIGNATURE_BASE64)
            .expect("base64 signature"),
        TEST_MINISIGN_SIGNATURE
    );
}

#[test]
fn decode_tauri_minisign_signature_rejects_invalid_payloads() {
    let bad_base64 = decode_tauri_minisign_signature("not a tauri updater signature!");
    assert!(matches!(
        bad_base64,
        Err(UpdaterError::VerificationFailed(message))
            if message.contains("base64 decode failed")
    ));

    let non_minisign = decode_tauri_minisign_signature("dGVzdA==");
    assert!(matches!(
        non_minisign,
        Err(UpdaterError::VerificationFailed(message))
            if message.contains("not a minisign signature")
    ));
}

#[test]
fn check_returns_none_without_manifest_source() {
    let (tx, _rx) = unbounded::<UpdateEvent>();
    let updater = Updater::with_manifest_source(tx, None);
    assert!(updater.check().expect("check").is_none());
}

#[test]
fn check_reads_local_manifest_and_returns_available_update() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-manifest-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &manifest_path,
        r#"{"version":"9.9.9","date":"2026-05-11","body":"Test release"}"#,
    )
    .expect("write manifest");
    let (tx, _rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));

    let info = updater.check().expect("check").expect("available");
    assert_eq!(info.version.as_str(), "9.9.9");
    assert_eq!(info.current_version.as_str(), env!("CARGO_PKG_VERSION"));
    assert_eq!(info.date.as_deref(), Some("2026-05-11"));
    assert_eq!(info.body.as_deref(), Some("Test release"));
    let _ = std::fs::remove_file(&manifest_path);
}

#[test]
fn check_honours_skipped_manifest_version() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-skipped-{}.json",
        std::process::id()
    ));
    std::fs::write(&manifest_path, r#"{"version":"9.9.8"}"#).expect("write manifest");
    let (tx, _rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
    updater.skip_version(SmolStr::new_static("9.9.8"));

    assert!(updater.check().expect("check").is_none());
    let _ = std::fs::remove_file(&manifest_path);
}

#[test]
fn check_tauri_style_manifest_maps_notes_and_pub_date() {
    let info = parse_update_manifest(
        r#"{"version":"9.9.7","pub_date":"2026-05-11T00:00:00Z","notes":"Release notes","url":"file://C:/tmp/BentoDeskSetup.exe","sha256":"204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df","signature":"sig"}"#,
        SmolStr::new_static("0.0.1"),
    )
    .expect("manifest");

    assert_eq!(info.version.as_str(), "9.9.7");
    assert_eq!(info.current_version.as_str(), "0.0.1");
    assert_eq!(info.date.as_deref(), Some("2026-05-11T00:00:00Z"));
    assert_eq!(info.body.as_deref(), Some("Release notes"));
    assert_eq!(
        info.artifact_url.as_deref(),
        Some("file://C:/tmp/BentoDeskSetup.exe")
    );
    assert_eq!(
        info.artifact_sha256.as_deref(),
        Some("204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df")
    );
    assert_eq!(info.signature.as_deref(), Some("sig"));
}

#[test]
fn check_tauri_v2_platform_manifest_selects_windows_artifact() {
    let info = parse_update_manifest(
        r#"{
            "version":"9.9.7",
            "pub_date":"2026-05-11T00:00:00Z",
            "notes":"Release notes",
            "url":"file://C:/tmp/FallbackSetup.exe",
            "sha256":"0000000000000000000000000000000000000000000000000000000000000000",
            "platforms":{
                "darwin-aarch64":{
                    "url":"https://example.invalid/BentoDesk.dmg",
                    "signature":"mac-sig"
                },
                "windows-x86_64":{
                    "url":"file://C:/tmp/BentoDeskSetup.exe",
                    "signature":"win-sig",
                    "sha256":"204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df"
                }
            }
        }"#,
        SmolStr::new_static("0.0.1"),
    )
    .expect("platform manifest");

    assert_eq!(
        info.artifact_url.as_deref(),
        Some("file://C:/tmp/BentoDeskSetup.exe")
    );
    assert_eq!(
        info.artifact_sha256.as_deref(),
        Some("204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df")
    );
    assert_eq!(info.signature.as_deref(), Some("win-sig"));
}

#[test]
fn version_compare_handles_semver_and_equal_versions() {
    assert!(version_is_newer("1.2.4", "1.2.3"));
    assert!(version_is_newer("v2.0.0", "1.9.9"));
    assert!(!version_is_newer("1.2.3", "1.2.3"));
    assert!(!version_is_newer("1.2.2", "1.2.3"));
}

#[test]
fn download_copies_local_artifact_and_emits_progress_ready() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-download-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"installer-bytes").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.6","artifact_url":"{artifact_source}","artifact_sha256":"204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df"}}"#
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
    assert!(updater.check().expect("check").is_some());

    updater.download().expect("download");
    let staged = updater.staged_artifact().expect("staged artifact");
    assert_eq!(
        std::fs::read(&staged).expect("read staged"),
        b"installer-bytes"
    );

    let mut saw_progress = false;
    let mut saw_ready = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            UpdateEvent::Progress { progress } => {
                saw_progress = true;
                assert_eq!(progress.total_bytes, Some(15));
                assert_eq!(progress.chunk_len, 15);
            }
            UpdateEvent::Ready { info } => {
                saw_ready = true;
                assert_eq!(info.version.as_str(), "9.9.6");
            }
            other => panic!("unexpected updater event {other:?}"),
        }
    }
    assert!(saw_progress);
    assert!(saw_ready);
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn download_copies_tauri_v2_platform_artifact_and_emits_progress_ready() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-platform-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-platform-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"installer-bytes").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{
                "version":"9.9.55",
                "platforms":{{
                    "windows-x86_64":{{
                        "url":"{artifact_source}",
                        "sha256":"204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df"
                    }}
                }}
            }}"#
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
    assert!(updater.check().expect("check").is_some());

    updater.download().expect("download");
    let staged = updater.staged_artifact().expect("staged artifact");
    assert_eq!(
        std::fs::read(&staged).expect("read staged"),
        b"installer-bytes"
    );

    let mut saw_progress = false;
    let mut saw_ready = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            UpdateEvent::Progress { progress } => {
                saw_progress = true;
                assert_eq!(progress.total_bytes, Some(15));
                assert_eq!(progress.chunk_len, 15);
            }
            UpdateEvent::Ready { info } => {
                saw_ready = true;
                assert_eq!(info.version.as_str(), "9.9.55");
                assert_eq!(info.signature.as_deref(), None);
            }
            other => panic!("unexpected updater event {other:?}"),
        }
    }
    assert!(saw_progress);
    assert!(saw_ready);
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
    let _ = std::fs::remove_file(&staged);
}

#[cfg(windows)]
#[test]
fn download_accepts_valid_minisign_signature_with_sha256() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-signed-sha-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-signed-sha-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"test").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.4","artifact_url":"{artifact_source}","sha256":"{TEST_ARTIFACT_SHA256}","signature":{}}}"#,
            serde_json::to_string(TEST_MINISIGN_SIGNATURE).expect("signature json")
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater = updater_with_test_minisign_key(tx, &manifest_path);
    assert!(updater.check().expect("check").is_some());

    updater.download().expect("download");
    let staged = updater.staged_artifact().expect("staged artifact");
    assert_eq!(std::fs::read(&staged).expect("read staged"), b"test");
    assert!(rx.try_iter().any(
        |event| matches!(event, UpdateEvent::Ready { info } if info.version.as_str() == "9.9.4")
    ));
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
    let _ = std::fs::remove_file(&staged);
}

#[cfg(windows)]
#[test]
fn download_accepts_valid_minisign_signature_only_manifest() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-signature-only-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-signature-only-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"test").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.3","artifact_url":"{artifact_source}","signature":{}}}"#,
            serde_json::to_string(TEST_MINISIGN_SIGNATURE).expect("signature json")
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater = updater_with_test_minisign_key(tx, &manifest_path);
    assert!(updater.check().expect("check").is_some());

    updater.download().expect("download");
    let staged = updater.staged_artifact().expect("staged artifact");
    assert_eq!(std::fs::read(&staged).expect("read staged"), b"test");
    assert!(rx.try_iter().any(
        |event| matches!(event, UpdateEvent::Ready { info } if info.version.as_str() == "9.9.3")
    ));
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
    let _ = std::fs::remove_file(&staged);
}

#[cfg(windows)]
#[test]
fn download_accepts_tauri_base64_minisign_signature() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-base64-signature-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-base64-signature-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"test").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.32","artifact_url":"{artifact_source}","signature":{}}}"#,
            serde_json::to_string(TEST_MINISIGN_SIGNATURE_BASE64).expect("signature json")
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater = updater_with_test_minisign_key(tx, &manifest_path);
    assert!(updater.check().expect("check").is_some());

    updater.download().expect("download");
    let staged = updater.staged_artifact().expect("staged artifact");
    assert_eq!(std::fs::read(&staged).expect("read staged"), b"test");
    assert!(rx.try_iter().any(
        |event| matches!(event, UpdateEvent::Ready { info } if info.version.as_str() == "9.9.32")
    ));
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
    let _ = std::fs::remove_file(&staged);
}

#[cfg(windows)]
#[test]
fn download_verifies_tauri_v2_platform_minisign_signature() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-platform-signature-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-platform-signature-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"test").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{
                "version":"9.9.56",
                "platforms":{{
                    "windows-x86_64":{{
                        "url":"{artifact_source}",
                        "signature":{}
                    }}
                }}
            }}"#,
            serde_json::to_string(TEST_MINISIGN_SIGNATURE).expect("signature json")
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater = updater_with_test_minisign_key(tx, &manifest_path);
    assert!(updater.check().expect("check").is_some());

    updater.download().expect("download");
    let staged = updater.staged_artifact().expect("staged artifact");
    assert_eq!(std::fs::read(&staged).expect("read staged"), b"test");
    assert!(rx.try_iter().any(
        |event| matches!(event, UpdateEvent::Ready { info } if info.version.as_str() == "9.9.56")
    ));
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
    let _ = std::fs::remove_file(&staged);
}

#[cfg(windows)]
#[test]
fn download_verifies_tauri_v2_platform_base64_minisign_signature() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-platform-base64-signature-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-platform-base64-signature-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"test").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{
                "version":"9.9.57",
                "platforms":{{
                    "windows-x86_64":{{
                        "url":"{artifact_source}",
                        "signature":{}
                    }}
                }}
            }}"#,
            serde_json::to_string(TEST_MINISIGN_SIGNATURE_BASE64).expect("signature json")
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater = updater_with_test_minisign_key(tx, &manifest_path);
    assert!(updater.check().expect("check").is_some());

    updater.download().expect("download");
    let staged = updater.staged_artifact().expect("staged artifact");
    assert_eq!(std::fs::read(&staged).expect("read staged"), b"test");
    assert!(rx.try_iter().any(
        |event| matches!(event, UpdateEvent::Ready { info } if info.version.as_str() == "9.9.57")
    ));
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
    let _ = std::fs::remove_file(&staged);
}

#[cfg(windows)]
#[test]
fn download_deletes_stage_and_emits_error_when_minisign_mismatches() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-signature-mismatch-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-signature-mismatch-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"Test").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.31","artifact_url":"{artifact_source}","signature":{}}}"#,
            serde_json::to_string(TEST_MINISIGN_SIGNATURE).expect("signature json")
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater = updater_with_test_minisign_key(tx, &manifest_path);
    assert!(updater.check().expect("check").is_some());

    let error = updater
        .download()
        .expect_err("artifact signed for different bytes must fail");
    assert!(matches!(error, UpdaterError::VerificationFailed(_)));
    assert!(updater.staged_artifact().is_none());
    let staged = staged_artifact_path("9.9.31", artifact_source.as_str()).expect("stage path");
    assert!(!staged.exists());
    let event = rx
        .try_iter()
        .find(|event| matches!(event, UpdateEvent::Error { .. }))
        .expect("verify error event");
    assert!(matches!(
        event,
        UpdateEvent::Error { kind, message }
            if kind.as_str() == "verify" && message.contains("minisign signature mismatch")
    ));
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
}

#[cfg(windows)]
#[test]
fn download_deletes_stage_and_emits_error_when_base64_signature_is_invalid() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-invalid-base64-signature-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-invalid-base64-signature-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"test").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.33","artifact_url":"{artifact_source}","signature":"not a tauri updater signature!"}}"#
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater = updater_with_test_minisign_key(tx, &manifest_path);
    assert!(updater.check().expect("check").is_some());

    let error = updater
        .download()
        .expect_err("invalid base64 signature must fail");
    assert!(matches!(error, UpdaterError::VerificationFailed(_)));
    assert!(updater.staged_artifact().is_none());
    let staged = staged_artifact_path("9.9.33", artifact_source.as_str()).expect("stage path");
    assert!(!staged.exists());
    let event = rx
        .try_iter()
        .find(|event| matches!(event, UpdateEvent::Error { .. }))
        .expect("verify error event");
    assert!(matches!(
        event,
        UpdateEvent::Error { kind, message }
            if kind.as_str() == "verify" && message.contains("base64 decode failed")
    ));
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
}

#[cfg(windows)]
#[test]
fn download_deletes_stage_and_emits_error_when_signed_sha256_mismatches() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-sha-mismatch-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-sha-mismatch-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"test").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.2","artifact_url":"{artifact_source}","sha256":"0000000000000000000000000000000000000000000000000000000000000000","signature":{}}}"#,
            serde_json::to_string(TEST_MINISIGN_SIGNATURE).expect("signature json")
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater = updater_with_test_minisign_key(tx, &manifest_path);
    assert!(updater.check().expect("check").is_some());

    let error = updater.download().expect_err("sha mismatch must fail");
    assert!(matches!(error, UpdaterError::VerificationFailed(_)));
    assert!(updater.staged_artifact().is_none());
    let staged = staged_artifact_path("9.9.2", artifact_source.as_str()).expect("stage path");
    assert!(!staged.exists());
    let event = rx
        .try_iter()
        .find(|event| matches!(event, UpdateEvent::Error { .. }))
        .expect("verify error event");
    assert!(matches!(
        event,
        UpdateEvent::Error { kind, message }
            if kind.as_str() == "verify" && message.contains("sha256 mismatch")
    ));
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
}

#[cfg(windows)]
#[test]
fn background_check_emits_available_and_preserves_pending_download() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bento-nano-update-background-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bento-nano-update-background-artifact-{}.bin",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"installer-bytes").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.1","artifact_url":"{artifact_source}","sha256":"204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df"}}"#
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));

    updater.spawn_background_check();
    let event = rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("background updater event");
    assert!(matches!(
        event,
        UpdateEvent::Available { info } if info.version.as_str() == "9.9.1"
    ));

    updater.download().expect("download after background check");
    let staged = updater.staged_artifact().expect("staged artifact");
    assert_eq!(
        std::fs::read(&staged).expect("read staged"),
        b"installer-bytes"
    );
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
    let _ = std::fs::remove_file(&staged);
}
