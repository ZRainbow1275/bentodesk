#[test]
fn updater_up_to_date_event_applies_to_checking_or_stale_available_only() {
    let app = AppState::new();
    *app.settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Skipped {
        version: SmolStr::new_static("2.1.0"),
    };
    apply_update_event_to_app(
        &app,
        UpdateEvent::UpToDate {
            current_version: SmolStr::new_static("2.1.0"),
        },
    );
    assert!(matches!(
        *app.settings_updater_status.borrow(),
        SettingsUpdaterStatus::Skipped { .. }
    ));

    *app.settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Available {
        version: SmolStr::new_static("2.0.9"),
    };
    apply_update_event_to_app(
        &app,
        UpdateEvent::UpToDate {
            current_version: SmolStr::new_static("2.1.0"),
        },
    );
    assert!(matches!(
        *app.settings_updater_status.borrow(),
        SettingsUpdaterStatus::UpToDate { .. }
    ));

    *app.settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Checking;
    apply_update_event_to_app(
        &app,
        UpdateEvent::UpToDate {
            current_version: SmolStr::new_static("2.1.0"),
        },
    );
    assert_eq!(
        *app.settings_updater_status.borrow(),
        SettingsUpdaterStatus::UpToDate {
            current_version: SmolStr::new_static("2.1.0")
        }
    );
}

#[test]
fn updater_recurring_check_clears_stale_available_after_download_rejection() {
    let (tx, rx) = crossbeam_channel::unbounded::<UpdateEvent>();
    let base = test_app_root();
    let root = AppRoot {
        updater: bentodesk_backend::updater::Updater::with_manifest_source(tx, None),
        updater_events: rx,
        ..base
    };
    *root.app.borrow().settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Available {
        version: SmolStr::new_static("2.0.9"),
    };
    assert_eq!(
        root.updater.try_start_check(),
        bentodesk_backend::updater::UpdateStart::Started
    );
    assert_eq!(
        root.updater.try_start_download(),
        bentodesk_backend::updater::UpdateStart::Rejected(
            bentodesk_backend::updater::UpdateOperation::Checking
        )
    );
    for _ in 0..200 {
        if !root.updater_events.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(super::drain_backend_events(&root));
    assert!(matches!(
        *root.app.borrow().settings_updater_status.borrow(),
        SettingsUpdaterStatus::UpToDate { .. }
    ));
    assert_eq!(
        root.updater.operation_state(),
        bentodesk_backend::updater::UpdateOperation::Idle
    );
}

#[test]
fn updater_queued_terminal_stays_owned_until_ui_pump_acknowledges_it() {
    let manifest = std::env::temp_dir().join(format!(
        "bentodesk-shell-updater-queued-terminal-{}.json",
        std::process::id()
    ));
    std::fs::write(&manifest, r#"{"version":"9.9.83"}"#).expect("manifest");
    let (tx, rx) = crossbeam_channel::unbounded::<UpdateEvent>();
    let base = test_app_root();
    let root = AppRoot {
        updater: bentodesk_backend::updater::Updater::with_manifest_source(
            tx,
            Some(SmolStr::new(manifest.to_string_lossy())),
        ),
        updater_events: rx,
        ..base
    };
    root.app.borrow().update_auto_download.set(false);
    *root.app.borrow().settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Checking;
    assert_eq!(
        root.updater.try_start_check(),
        bentodesk_backend::updater::UpdateStart::Started
    );
    for _ in 0..200 {
        if root.updater_events.len() == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(root.updater_events.len(), 1, "one terminal queued before pump");
    assert_eq!(
        *root.app.borrow().settings_updater_status.borrow(),
        SettingsUpdaterStatus::Checking
    );
    assert_eq!(
        root.updater.operation_state(),
        bentodesk_backend::updater::UpdateOperation::Checking
    );
    assert_eq!(
        root.updater.try_start_download(),
        bentodesk_backend::updater::UpdateStart::Rejected(
            bentodesk_backend::updater::UpdateOperation::Checking
        )
    );

    assert!(super::drain_backend_events(&root));
    assert!(matches!(
        *root.app.borrow().settings_updater_status.borrow(),
        SettingsUpdaterStatus::Available { ref version } if version == "9.9.83"
    ));
    assert_eq!(
        root.updater.operation_state(),
        bentodesk_backend::updater::UpdateOperation::Idle
    );
    assert!(root.updater_events.is_empty());

    let _ = std::fs::remove_file(manifest);
}

#[test]
fn updater_idle_backend_pump_acks_terminal_and_suppresses_skipped_available() {
    let manifest = std::env::temp_dir().join(format!(
        "bentodesk-shell-updater-stale-{}.json",
        std::process::id()
    ));
    std::fs::write(&manifest, r#"{"version":"9.9.81"}"#).expect("manifest");
    let (tx, rx) = crossbeam_channel::unbounded::<UpdateEvent>();
    let base = test_app_root();
    let root = AppRoot {
        updater: bentodesk_backend::updater::Updater::with_manifest_source(
            tx,
            Some(SmolStr::new(manifest.to_string_lossy())),
        ),
        updater_events: rx,
        ..base
    };
    assert_eq!(
        root.updater.try_start_check(),
        bentodesk_backend::updater::UpdateStart::Started
    );
    for _ in 0..200 {
        if !root.updater_events.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!root.updater_events.is_empty(), "check terminal queued");
    root.updater.skip_version(SmolStr::new_static("9.9.81"));
    {
        let app = root.app.borrow();
        app.update_auto_download.set(true);
        *app.settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Skipped {
            version: SmolStr::new_static("9.9.81"),
        };
    }

    assert!(super::drain_backend_events(&root));
    assert_eq!(
        root.updater.operation_state(),
        bentodesk_backend::updater::UpdateOperation::Idle
    );
    assert!(matches!(
        *root.app.borrow().settings_updater_status.borrow(),
        SettingsUpdaterStatus::Skipped { .. }
    ));
    assert!(root.updater_events.is_empty());

    let timer_source = include_str!("../shell/main_window_timer.rs");
    let poll_branch = timer_source
        .split("if wparam == BACKEND_EVENT_POLL_TIMER_ID")
        .nth(1)
        .expect("permanent backend poll branch");
    assert!(poll_branch.contains("drain_backend_events(root)"));
    assert!(poll_branch.contains("request_redraw(hwnd)"));

    let _ = std::fs::remove_file(manifest);
}

#[test]
fn updater_auto_download_starts_worker_and_reaches_ready_through_same_pump() {
    let manifest = std::env::temp_dir().join(format!(
        "bentodesk-shell-updater-auto-{}.json",
        std::process::id()
    ));
    let artifact = std::env::temp_dir().join(format!(
        "bentodesk-shell-updater-auto-{}.exe",
        std::process::id()
    ));
    std::fs::write(&artifact, b"test").expect("artifact");
    let source = artifact.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest,
        format!(
            r#"{{"version":"9.9.82","artifact_url":"{source}","artifact_sha256":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"}}"#
        ),
    )
    .expect("manifest");
    let (tx, rx) = crossbeam_channel::unbounded::<UpdateEvent>();
    let base = test_app_root();
    let root = AppRoot {
        updater: bentodesk_backend::updater::Updater::with_manifest_source(
            tx,
            Some(SmolStr::new(manifest.to_string_lossy())),
        ),
        updater_events: rx,
        ..base
    };
    root.app.borrow().update_auto_download.set(true);
    assert_eq!(
        root.updater.try_start_check(),
        bentodesk_backend::updater::UpdateStart::Started
    );
    for _ in 0..400 {
        let _ = super::drain_backend_events(&root);
        if matches!(
            *root.app.borrow().settings_updater_status.borrow(),
            SettingsUpdaterStatus::Ready { .. }
        ) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(matches!(
        *root.app.borrow().settings_updater_status.borrow(),
        SettingsUpdaterStatus::Ready { ref version } if version == "9.9.82"
    ));
    assert_eq!(
        root.updater.operation_state(),
        bentodesk_backend::updater::UpdateOperation::Ready
    );
    root.updater.skip_version(SmolStr::new_static("9.9.82"));
    assert_eq!(
        root.updater.operation_state(),
        bentodesk_backend::updater::UpdateOperation::Idle
    );

    let _ = std::fs::remove_file(manifest);
    let _ = std::fs::remove_file(artifact);
}
