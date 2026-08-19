fn github_release_value(tag: &str) -> serde_json::Value {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    let name = format!("BentoDesk-{version}-windows-x64-setup.exe");
    serde_json::json!({
        "tag_name": tag,
        "draft": false,
        "prerelease": false,
        "immutable": true,
        "published_at": "2026-08-16T00:00:00Z",
        "body": "release notes",
        "assets": [{
            "name": name,
            "state": "uploaded",
            "size": 1_202_628,
            "digest": format!("sha256:{}", "A".repeat(64)),
            "browser_download_url": format!(
                "https://github.com/ZRainbow1275/bentodesk/releases/download/{tag}/{name}"
            )
        }]
    })
}

#[test]
fn github_release_adapter_accepts_only_exact_immutable_setup_identity() {
    let text = github_release_value("v2.1.0").to_string();
    let info =
        parse_update_manifest(&text, SmolStr::new_static("2.0.10")).expect("strict GitHub release");
    assert_eq!(info.version.as_str(), "2.1.0");
    assert_eq!(info.current_version.as_str(), "2.0.10");
    assert_eq!(
        info.artifact_sha256.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(
        info.artifact_url.as_deref(),
        Some(
            "https://github.com/ZRainbow1275/bentodesk/releases/download/v2.1.0/\
             BentoDesk-2.1.0-windows-x64-setup.exe"
        )
    );
}

#[test]
fn github_release_adapter_rejects_release_asset_and_digest_drift() {
    let mut cases = Vec::new();

    let mut draft = github_release_value("v2.1.0");
    draft["draft"] = true.into();
    cases.push(draft);

    let mut prerelease = github_release_value("v2.1.0");
    prerelease["prerelease"] = true.into();
    cases.push(prerelease);

    let mut mutable = github_release_value("v2.1.0");
    mutable["immutable"] = false.into();
    cases.push(mutable);

    cases.push(github_release_value("v02.1.0"));
    cases.push(github_release_value("v2.1.0\n"));

    let mut pending = github_release_value("v2.1.0");
    pending["assets"][0]["state"] = "new".into();
    cases.push(pending);

    let mut empty = github_release_value("v2.1.0");
    empty["assets"][0]["size"] = 0.into();
    cases.push(empty);

    let mut oversized = github_release_value("v2.1.0");
    oversized["assets"][0]["size"] = (MAX_UPDATE_ARTIFACT_BYTES + 1).into();
    cases.push(oversized);

    let mut missing_digest = github_release_value("v2.1.0");
    missing_digest["assets"][0]["digest"] = serde_json::Value::Null;
    cases.push(missing_digest);

    let mut malformed_digest = github_release_value("v2.1.0");
    malformed_digest["assets"][0]["digest"] = format!("sha256:{}", "g".repeat(64)).into();
    cases.push(malformed_digest);

    let mut wrong_url = github_release_value("v2.1.0");
    wrong_url["assets"][0]["browser_download_url"] =
        "http://github.com/ZRainbow1275/bentodesk/releases/download/v2.1.0/setup.exe".into();
    cases.push(wrong_url);

    let mut duplicate = github_release_value("v2.1.0");
    let asset = duplicate["assets"][0].clone();
    duplicate["assets"]
        .as_array_mut()
        .expect("assets array")
        .push(asset);
    cases.push(duplicate);

    for value in cases {
        assert!(
            parse_update_manifest(&value.to_string(), SmolStr::new_static("2.0.0")).is_err(),
            "must reject {value}"
        );
    }
}

#[test]
fn canonical_semver_comparison_is_strict_and_does_not_overflow() {
    assert!(version_is_newer(
        "184467440737095516160.0.0",
        "184467440737095516159.999.999"
    ));
    assert!(!version_is_newer(
        "184467440737095516159.999.999",
        "184467440737095516160.0.0"
    ));
    assert!(github::canonical_parts("0.1.2").is_some());
    for invalid in ["02.1.0", "2.1", "2.1.0 ", "2.1.0-alpha", "V2.1.0"] {
        assert!(github::canonical_parts(invalid).is_none(), "{invalid}");
    }
}

#[test]
fn updater_new_defaults_to_official_latest_when_override_is_absent() {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let previous = std::env::var_os(MANIFEST_ENV);
    // SAFETY: this test serializes mutation of the updater environment variable
    // and restores the exact prior value before returning.
    unsafe { std::env::remove_var(MANIFEST_ENV) };
    let (tx, _rx) = unbounded::<UpdateEvent>();
    let updater = Updater::new(tx);
    assert_eq!(
        updater.manifest_source.as_deref(),
        Some(OFFICIAL_LATEST_RELEASE_URL)
    );
    // SAFETY: same serialized test scope; restore the original process value.
    unsafe {
        match previous {
            Some(value) => std::env::set_var(MANIFEST_ENV, value),
            None => std::env::remove_var(MANIFEST_ENV),
        }
    }
}

#[test]
fn up_to_date_event_serde_round_trip_is_additive() {
    let event = UpdateEvent::UpToDate {
        current_version: SmolStr::new_static("2.1.0"),
    };
    let encoded = serde_json::to_string(&event).expect("serialize");
    let decoded: UpdateEvent = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(decoded, event);
}

#[test]
fn background_check_gate_waits_for_terminal_pump_ack() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bentodesk-update-gate-check-{}.json",
        std::process::id()
    ));
    std::fs::write(&manifest_path, r#"{"version":"9.9.71"}"#).expect("manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));

    assert_eq!(updater.try_start_check(), UpdateStart::Started);
    assert_eq!(
        updater.try_start_check(),
        UpdateStart::Joined(UpdateOperation::Checking)
    );
    let event = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("check terminal");
    assert!(matches!(event, UpdateEvent::Available { .. }));
    assert_eq!(updater.operation_state(), UpdateOperation::Checking);
    assert!(matches!(
        updater.install_with_launcher(|_| Ok(())),
        Err(UpdaterError::InvalidManifest(message))
            if message.contains("cannot install while updater operation is Checking")
    ));
    assert_eq!(
        updater.try_start_download(),
        UpdateStart::Rejected(UpdateOperation::Checking)
    );
    updater.acknowledge_event(&event);
    assert_eq!(updater.operation_state(), UpdateOperation::Idle);

    let _ = std::fs::remove_file(manifest_path);
}

#[test]
fn worker_spawn_failure_keeps_gate_until_terminal_ack_or_send_failure() {
    for (owned, kind) in [
        (UpdateOperation::Checking, "check"),
        (UpdateOperation::Downloading, "download"),
    ] {
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater = Updater::with_manifest_source(tx, None);
        assert_eq!(
            updater.acquire_operation_for_test(owned),
            UpdateStart::Started
        );
        updater.finish_worker_spawn_for_test(owned, kind);
        let event = rx.recv_timeout(Duration::from_secs(1)).expect("spawn failure");
        assert!(matches!(
            &event,
            UpdateEvent::Error { kind: event_kind, message }
                if event_kind == kind && message.contains("worker spawn failure")
        ));
        assert_eq!(updater.operation_state(), owned);
        let competing = match owned {
            UpdateOperation::Checking => updater.try_start_download(),
            UpdateOperation::Downloading => updater.try_start_check(),
            _ => unreachable!("test only owns a worker operation"),
        };
        assert_eq!(competing, UpdateStart::Rejected(owned));
        updater.acknowledge_event(&event);
        assert_eq!(updater.operation_state(), UpdateOperation::Idle);
    }

    for (owned, kind) in [
        (UpdateOperation::Checking, "check"),
        (UpdateOperation::Downloading, "download"),
    ] {
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater = Updater::with_manifest_source(tx, None);
        drop(rx);
        assert_eq!(
            updater.acquire_operation_for_test(owned),
            UpdateStart::Started
        );
        updater.finish_worker_spawn_for_test(owned, kind);
        assert_eq!(updater.operation_state(), UpdateOperation::Idle);
    }
}

#[test]
fn recurring_scheduler_cancels_manual_and_reconfigures_without_old_period() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bentodesk-update-scheduler-reconfigure-{}.json",
        std::process::id()
    ));
    std::fs::write(&manifest_path, r#"{"version":"9.9.73"}"#).expect("manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));

    updater.configure_scheduler_for_test(Some(Duration::from_millis(20)));
    let first = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("initial scheduler check");
    assert!(matches!(first, UpdateEvent::Available { .. }));
    updater.acknowledge_event(&first);

    updater.configure_scheduler_for_test(None);
    assert!(
        rx.recv_timeout(Duration::from_millis(80)).is_err(),
        "Manual must cancel the next scheduled check"
    );

    updater.configure_scheduler_for_test(Some(Duration::from_millis(20)));
    let enabled = rx
        .recv_timeout(Duration::from_millis(80))
        .expect("re-enabled scheduler checks immediately");
    updater.acknowledge_event(&enabled);

    updater.configure_scheduler_for_test(Some(Duration::from_millis(180)));
    assert!(
        rx.recv_timeout(Duration::from_millis(90)).is_err(),
        "reconfiguration must not retain the old period"
    );
    let reconfigured = rx
        .recv_timeout(Duration::from_millis(220))
        .expect("new period check");
    updater.acknowledge_event(&reconfigured);
    updater.configure_scheduler_for_test(None);

    let _ = std::fs::remove_file(manifest_path);
}

#[test]
fn unrelated_scheduler_error_cannot_ack_an_active_check() {
    let (tx, _rx) = unbounded::<UpdateEvent>();
    let updater = Updater::with_manifest_source(tx, None);
    assert_eq!(updater.try_start_check(), UpdateStart::Started);
    updater.acknowledge_event(&UpdateEvent::Error {
        kind: SmolStr::new_static("scheduler"),
        message: "scheduler spawn failed".to_owned(),
    });
    assert_eq!(updater.operation_state(), UpdateOperation::Checking);
}

#[test]
fn ready_generation_rejects_check_and_only_matching_skip_releases_it() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bentodesk-update-ready-gate-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bentodesk-update-ready-gate-{}.exe",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"test").expect("artifact");
    let source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.72","artifact_url":"{source}","artifact_sha256":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"}}"#
        ),
    )
    .expect("manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
    assert!(updater.check().expect("check").is_some());
    assert_eq!(updater.try_start_download(), UpdateStart::Started);
    let ready = loop {
        let event = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("download event");
        if matches!(event, UpdateEvent::Ready { .. }) {
            break event;
        }
    };
    assert_eq!(updater.operation_state(), UpdateOperation::Downloading);
    updater.acknowledge_event(&ready);
    assert_eq!(updater.operation_state(), UpdateOperation::Ready);
    let staged = updater.staged_artifact().expect("staged");
    assert_eq!(
        updater.try_start_check(),
        UpdateStart::Rejected(UpdateOperation::Ready)
    );

    updater.skip_version(SmolStr::new_static("9.9.999"));
    assert_eq!(updater.operation_state(), UpdateOperation::Ready);
    assert_eq!(updater.staged_artifact().as_deref(), Some(staged.as_path()));

    updater.skip_version(SmolStr::new_static("9.9.72"));
    assert_eq!(updater.operation_state(), UpdateOperation::Idle);
    assert!(updater.staged_artifact().is_none());
    assert!(!staged.exists());

    assert_eq!(updater.try_start_check(), UpdateStart::Started);
    let next_check = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("post-skip check terminal");
    assert!(matches!(next_check, UpdateEvent::UpToDate { .. }));
    updater.acknowledge_event(&next_check);
    assert_eq!(updater.operation_state(), UpdateOperation::Idle);

    let _ = std::fs::remove_file(manifest_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn download_progress_is_bounded_for_known_and_unknown_short_reads() {
    for total in [Some(MAX_UPDATE_ARTIFACT_BYTES), None] {
        let (tx, rx) = unbounded::<UpdateEvent>();
        let mut progress = DownloadProgress::new(&tx, total);
        let mut written = 0u64;
        while written < MAX_UPDATE_ARTIFACT_BYTES {
            written = written
                .saturating_add(DOWNLOAD_BUFFER_BYTES as u64)
                .min(MAX_UPDATE_ARTIFACT_BYTES);
            progress.observe(written).expect("progress");
        }
        progress.finish(written).expect("final progress");
        let events = rx.try_iter().collect::<Vec<_>>();
        assert!(events.len() <= MAX_PROGRESS_EVENTS as usize);
        assert!(matches!(
            events.last(),
            Some(UpdateEvent::Progress { progress })
                if progress.chunk_len == MAX_UPDATE_ARTIFACT_BYTES
        ));
    }
}

#[cfg(windows)]
#[test]
#[ignore = "explicit official GitHub network acceptance"]
fn github_latest_release_check_and_download_match_digest() {
    let stage = std::env::temp_dir().join(format!(
        "bentodesk-official-network-proof-{}.exe",
        std::process::id()
    ));
    let result = (|| {
        let text = winhttp::fetch_text(
            OFFICIAL_LATEST_RELEASE_URL,
            MAX_MANIFEST_BYTES,
            MANIFEST_FETCH_DEADLINE,
        )?;
        let info = parse_update_manifest(&text, pkg_version())?;
        let url = info.artifact_url.as_deref().ok_or_else(|| {
            UpdaterError::InvalidManifest("official release missing artifact URL".to_owned())
        })?;
        let (tx, _rx) = unbounded::<UpdateEvent>();
        let bytes = winhttp::download_to_stage(
            url,
            &stage,
            MAX_UPDATE_ARTIFACT_BYTES,
            ARTIFACT_DOWNLOAD_DEADLINE,
            &tx,
        )?;
        verify_staged_artifact(&info, &stage, BENTODESK_UPDATE_MINISIGN_PUBLIC_KEY)?;
        eprintln!(
            "official_url={OFFICIAL_LATEST_RELEASE_URL} version={} asset={} bytes={} digest={}",
            info.version,
            url,
            bytes,
            info.artifact_sha256.as_deref().unwrap_or_default()
        );
        Ok::<(), UpdaterError>(())
    })();
    let _ = std::fs::remove_file(&stage);
    assert!(result.is_ok(), "{result:?}");
    assert!(!stage.exists());
}
