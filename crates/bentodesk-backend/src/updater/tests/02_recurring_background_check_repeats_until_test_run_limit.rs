#[test]
fn recurring_background_check_repeats_until_test_run_limit() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bentodesk-update-recurring-manifest-{}.json",
        std::process::id()
    ));
    std::fs::write(&manifest_path, r#"{"version":"9.9.2"}"#).expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));

    updater.spawn_recurring_background_check_for_test(Duration::from_millis(10), 2);
    for _ in 0..2 {
        let event = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("recurring updater event");
        assert!(matches!(
            event,
            UpdateEvent::Available { info } if info.version.as_str() == "9.9.2"
        ));
    }
    let _ = std::fs::remove_file(&manifest_path);
}

#[cfg(windows)]
#[test]
fn download_streams_http_artifact_and_emits_progress_ready() {
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::thread;

    let artifact_bytes = b"remote-installer-bytes".to_vec();
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local http");
    let addr = listener.local_addr().expect("local addr");
    let served_bytes = artifact_bytes.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request).expect("read request");
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            served_bytes.len()
        );
        stream.write_all(header.as_bytes()).expect("write header");
        stream.write_all(&served_bytes).expect("write body");
    });

    let manifest_path = std::env::temp_dir().join(format!(
        "bentodesk-update-http-download-manifest-{}.json",
        std::process::id()
    ));
    let artifact_url = format!("http://{addr}/BentoDeskSetup.exe");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.4","artifact_url":"{artifact_url}","artifact_sha256":"8a4afa450da6f852f01aafa1f9af741ec902acd96ef19f8cab1ada89dfb278f2"}}"#
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
    assert!(updater.check().expect("check").is_some());

    updater.download().expect("http download");
    server.join().expect("server join");
    let staged = updater.staged_artifact().expect("staged artifact");
    assert_eq!(std::fs::read(&staged).expect("read staged"), artifact_bytes);

    let mut saw_progress = false;
    let mut saw_ready = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            UpdateEvent::Progress { progress } => {
                saw_progress = true;
                assert_eq!(progress.total_bytes, Some(22));
                assert_eq!(progress.chunk_len, 22);
            }
            UpdateEvent::Ready { info } => {
                saw_ready = true;
                assert_eq!(info.version.as_str(), "9.9.4");
            }
            other => panic!("unexpected updater event {other:?}"),
        }
    }
    assert!(saw_progress);
    assert!(saw_ready);
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&staged);
}

#[cfg(windows)]
#[test]
fn updater_does_not_follow_http_redirects() {
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::thread;

    let target = TcpListener::bind(("127.0.0.1", 0)).expect("bind target");
    let target_addr = target.local_addr().expect("target addr");
    let target_server = thread::spawn(move || {
        target.set_nonblocking(true).expect("nonblocking");
        for _ in 0..100 {
            match target.accept() {
                Ok((mut stream, _)) => {
                    let mut request = [0u8; 1024];
                    let _ = stream.read(&mut request);
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                        )
                        .expect("target response");
                    return true;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("target accept: {error}"),
            }
        }
        false
    });

    let redirect = TcpListener::bind(("127.0.0.1", 0)).expect("bind redirect");
    let redirect_addr = redirect.local_addr().expect("redirect addr");
    let redirect_server = thread::spawn(move || {
        let (mut stream, _) = redirect.accept().expect("redirect accept");
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request).expect("redirect request");
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{target_addr}/update.json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream
            .write_all(response.as_bytes())
            .expect("redirect response");
    });

    let error = fetch_manifest_winhttp(&format!("http://{redirect_addr}/manifest.json"))
        .expect_err("redirect must not be followed");
    assert!(error.to_string().contains("HTTP 302"));
    redirect_server.join().expect("redirect server");
    assert!(!target_server.join().expect("target server"));
}

#[test]
fn install_requires_a_staged_artifact() {
    let (tx, _rx) = unbounded::<UpdateEvent>();
    let updater = Updater::with_manifest_source(tx, None);
    assert!(matches!(
        updater.install(),
        Err(UpdaterError::InvalidManifest(message)) if message.contains("no pending update")
    ));
}

#[test]
fn install_launches_staged_nsis_artifact_and_emits_installing() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bentodesk-update-install-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bentodesk-update-artifact-{}.exe",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"fake-nsis").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.5","artifact_url":"{artifact_source}","artifact_sha256":"e4d694994b0f50da53c20fbd386937836b3a772929c458f07f7d8a387c257c29"}}"#
        ),
    )
    .expect("write manifest");
    let (tx, rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
    assert!(updater.check().expect("check").is_some());
    updater.download().expect("download");
    let staged = updater.staged_artifact().expect("staged artifact");

    let launched = std::cell::RefCell::new(None::<PathBuf>);
    updater
        .install_with_launcher(|path| {
            launched.borrow_mut().replace(path.to_path_buf());
            Ok(())
        })
        .expect("install");
    assert_eq!(launched.borrow().as_ref(), Some(&staged));

    let mut saw_installing = false;
    while let Ok(event) = rx.try_recv() {
        if let UpdateEvent::Installing { info } = event {
            saw_installing = true;
            assert_eq!(info.version.as_str(), "9.9.5");
        }
    }
    assert!(saw_installing);
    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn install_rejects_a_staged_artifact_tampered_after_download() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bentodesk-update-tamper-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bentodesk-update-tamper-artifact-{}.exe",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"fake-nsis").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.6","artifact_url":"{artifact_source}","artifact_sha256":"e4d694994b0f50da53c20fbd386937836b3a772929c458f07f7d8a387c257c29"}}"#
        ),
    )
    .expect("write manifest");
    let (tx, _rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
    assert!(updater.check().expect("check").is_some());
    updater.download().expect("download");
    let staged = updater.staged_artifact().expect("staged artifact");
    std::fs::write(&staged, b"tampered").expect("tamper staged artifact");

    let launched = std::cell::Cell::new(false);
    let error = updater
        .install_with_launcher(|_| {
            launched.set(true);
            Ok(())
        })
        .expect_err("tampered artifact must not launch");
    assert!(matches!(error, UpdaterError::VerificationFailed(_)));
    assert!(!launched.get());

    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn a_new_manifest_invalidates_the_previous_staged_artifact() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bentodesk-update-generation-manifest-{}.json",
        std::process::id()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "bentodesk-update-generation-artifact-{}.exe",
        std::process::id()
    ));
    std::fs::write(&artifact_path, b"fake-nsis").expect("write artifact");
    let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{"version":"9.9.7","artifact_url":"{artifact_source}","artifact_sha256":"e4d694994b0f50da53c20fbd386937836b3a772929c458f07f7d8a387c257c29"}}"#
        ),
    )
    .expect("write manifest");
    let (tx, _rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
    assert!(updater.check().expect("first check").is_some());
    updater.download().expect("download");
    let old_staged = updater.staged_artifact().expect("old staged");

    std::fs::write(&manifest_path, r#"{"version":"9.9.8"}"#).expect("replace manifest");
    assert!(updater.check().expect("second check").is_some());
    assert!(updater.staged_artifact().is_none());
    assert!(!old_staged.exists());
    assert!(matches!(
        updater.install(),
        Err(UpdaterError::InvalidManifest(message)) if message.contains("no staged artifact")
    ));

    let _ = std::fs::remove_file(&manifest_path);
    let _ = std::fs::remove_file(&artifact_path);
}

#[test]
fn skip_version_round_trips() {
    let (tx, _rx) = unbounded::<UpdateEvent>();
    let updater = Updater::with_manifest_source(tx, None);
    assert!(updater.current_skipped().is_none());
    updater.skip_version(SmolStr::new_static("2.1.0"));
    assert_eq!(updater.current_skipped().as_deref(), Some("2.1.0"));
}

#[test]
fn check_interval_matches_frequency() {
    assert_eq!(check_interval_hours(UpdateCheckFrequency::Daily), Some(24));
    assert_eq!(
        check_interval_hours(UpdateCheckFrequency::Weekly),
        Some(24 * 7)
    );
    assert_eq!(check_interval_hours(UpdateCheckFrequency::Manual), None);
}

#[test]
fn pkg_version_matches_cargo_constant() {
    let v = pkg_version();
    assert_eq!(v.as_str(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn update_info_serde_round_trip() {
    let info = UpdateInfo {
        version: SmolStr::new_static("2.1.0"),
        current_version: SmolStr::new_static("2.0.0"),
        date: Some(SmolStr::new_static("2026-05-03T00:00:00Z")),
        body: Some("Initial v2.1 release".to_string()),
        artifact_url: Some("file://C:/tmp/BentoDeskSetup.exe".to_string()),
        artifact_sha256: Some(
            "204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df".to_string(),
        ),
        signature: Some("sig".to_string()),
    };
    let json = serde_json::to_string(&info).expect("serialize");
    let parsed: UpdateInfo = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, info);
}

#[test]
fn check_frequency_serde_round_trip() {
    let json = serde_json::to_string(&UpdateCheckFrequency::Daily).expect("ser");
    let parsed: UpdateCheckFrequency = serde_json::from_str(&json).expect("de");
    assert_eq!(parsed, UpdateCheckFrequency::Daily);
}
