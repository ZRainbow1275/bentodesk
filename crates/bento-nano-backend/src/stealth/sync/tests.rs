use super::*;

fn config_for(desktop: &Path, app_data: &Path) -> StealthConfig {
    StealthConfig {
        desktop_path: smol_str::SmolStr::new(desktop.to_string_lossy()),
        app_data_dir: smol_str::SmolStr::new(app_data.to_string_lossy()),
    }
}

struct TmpDir(PathBuf);
impl TmpDir {
    fn as_path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tempdir() -> TmpDir {
    let suffix = super::super::unique_suffix();
    let path = std::env::temp_dir().join(format!("bento-sync-{}-{}", std::process::id(), suffix));
    std::fs::create_dir_all(&path).expect("tempdir");
    TmpDir(path)
}

// ── parse_schema_version ───────────────────────────────────────

#[test]
fn schema_version_numeric_compare() {
    assert!(parse_schema_version("3.10") > parse_schema_version("3.9"));
    assert_eq!(parse_schema_version(""), (0, 0));
    assert_eq!(parse_schema_version("3.1"), (3, 1));
}

#[test]
fn needs_migration_detects_old_schema() {
    assert!(needs_migration(""));
    assert!(needs_migration("3.0"));
    assert!(!needs_migration("3.1"));
    assert!(!needs_migration("3.2"));
}

// ── manifest round-trip ─────────────────────────────────────────

#[test]
fn manifest_add_then_load_round_trip() {
    let tmp = tempdir();
    let dir = tmp.as_path();

    manifest_add(
        dir,
        ManifestAddParams {
            original_path: r"C:\Users\X\Desktop\foo.txt",
            hidden_path: r"C:\Users\X\Desktop\.bentodesk\z-1\foo.txt",
            zone_id: "z-1",
            file_size_bytes: 42,
            display_name: "foo.txt",
            icon_x: Some(10),
            icon_y: Some(20),
            file_type: "File",
        },
    )
    .expect("add");

    let m = load_manifest(dir).expect("load");
    assert_eq!(m.entries.len(), 1);
    assert_eq!(m.entries[0].original_path, r"C:\Users\X\Desktop\foo.txt");
    assert_eq!(m.entries[0].zone_id, "z-1");
    assert_eq!(m.entries[0].file_size_bytes, 42);
    assert_eq!(m.entries[0].icon_x, Some(10));
    assert_eq!(m.entries[0].icon_y, Some(20));
    assert_eq!(m.schema_version, MANIFEST_SCHEMA_VERSION);
}

#[test]
fn manifest_add_replaces_duplicate_original_path() {
    let tmp = tempdir();
    let dir = tmp.as_path();

    for hidden in ["a/foo.txt", "b/foo.txt"] {
        manifest_add(
            dir,
            ManifestAddParams {
                original_path: r"C:\foo.txt",
                hidden_path: hidden,
                zone_id: "z-1",
                file_size_bytes: 10,
                display_name: "foo.txt",
                icon_x: None,
                icon_y: None,
                file_type: "",
            },
        )
        .expect("add");
    }

    let m = load_manifest(dir).expect("load");
    assert_eq!(m.entries.len(), 1);
    assert_eq!(m.entries[0].hidden_path, "b/foo.txt");
}

#[test]
fn manifest_remove_by_original_path() {
    let tmp = tempdir();
    let dir = tmp.as_path();

    manifest_add(
        dir,
        ManifestAddParams {
            original_path: r"C:\a.txt",
            hidden_path: "h/a.txt",
            zone_id: "z",
            file_size_bytes: 1,
            display_name: "a.txt",
            icon_x: None,
            icon_y: None,
            file_type: "",
        },
    )
    .expect("add a");
    manifest_add(
        dir,
        ManifestAddParams {
            original_path: r"C:\b.txt",
            hidden_path: "h/b.txt",
            zone_id: "z",
            file_size_bytes: 1,
            display_name: "b.txt",
            icon_x: None,
            icon_y: None,
            file_type: "",
        },
    )
    .expect("add b");

    manifest_remove(dir, r"c:\A.TXT").expect("remove case-insensitive");

    let m = load_manifest(dir).expect("load");
    assert_eq!(m.entries.len(), 1);
    assert_eq!(m.entries[0].original_path, r"C:\b.txt");
}

#[test]
fn save_manifest_with_mirror_writes_both() {
    let tmp = tempdir();
    let desktop = tmp.as_path().join("desktop");
    let app_data = tmp.as_path().join("appdata");
    std::fs::create_dir_all(&desktop).expect("desktop");
    std::fs::create_dir_all(&app_data).expect("appdata");
    let cfg = config_for(&desktop, &app_data);

    let dir = desktop.join(".bentodesk");
    std::fs::create_dir_all(&dir).expect("hidden dir");

    let mut m = SafetyManifest::default();
    m.entries.push(ManifestEntry {
        original_path: "x".to_string(),
        hidden_path: "y".to_string(),
        zone_id: "z".to_string(),
        file_size_bytes: 0,
        hidden_at: now_iso8601(),
        display_name: "x".to_string(),
        icon_x: None,
        icon_y: None,
        file_type: "".to_string(),
    });

    save_manifest_with_mirror(&cfg, &dir, &m).expect("save");

    assert!(dir.join("manifest.json").exists());
    assert!(app_data.join("manifest.mirror.json").exists());
}

#[test]
fn write_json_atomic_creates_bak_on_overwrite() {
    let tmp = tempdir();
    let path = tmp.as_path().join("file.json");

    write_json_atomic(&path, &SafetyManifest::default()).expect("first write");
    let mut second = SafetyManifest::default();
    second.entries.push(ManifestEntry {
        original_path: "x".into(),
        hidden_path: "y".into(),
        zone_id: "z".into(),
        file_size_bytes: 0,
        hidden_at: now_iso8601(),
        display_name: "x".into(),
        icon_x: None,
        icon_y: None,
        file_type: "".into(),
    });
    write_json_atomic(&path, &second).expect("second write");

    assert!(path.exists());
    assert!(path.with_extension("json.bak").exists());
}

#[test]
fn read_json_with_recovery_restores_from_bak_on_corrupt_primary() {
    let tmp = tempdir();
    let path = tmp.as_path().join("file.json");

    write_json_atomic(&path, &SafetyManifest::default()).expect("seed");
    // Overwrite to create .bak.
    let mut updated = SafetyManifest::default();
    updated.entries.push(ManifestEntry {
        original_path: "x".into(),
        hidden_path: "y".into(),
        zone_id: "z".into(),
        file_size_bytes: 0,
        hidden_at: now_iso8601(),
        display_name: "x".into(),
        icon_x: None,
        icon_y: None,
        file_type: "".into(),
    });
    write_json_atomic(&path, &updated).expect("write 2");

    // Corrupt the primary.
    std::fs::write(&path, b"not json {{{").expect("corrupt");

    let recovered: Option<SafetyManifest> = read_json_with_recovery(&path).expect("recover");
    let recovered = recovered.expect("Some");
    // The .bak holds the previous (default-empty) manifest content
    // before updated was renamed in. Either is fine for this test,
    // both prove recovery worked without panic.
    assert_eq!(
        recovered.schema_version,
        MANIFEST_SCHEMA_VERSION.to_string()
    );
}

// ── AttrGuard worker pool ──────────────────────────────────────

#[test]
fn attr_guard_starts_and_drops_cleanly() {
    let guard = AttrGuard::start(None);
    // Just creating + dropping the guard exercises spawn + join
    // without panicking. The workers exit when shutdown=true and the
    // sender is replaced (channel disconnects).
    drop(guard);
}

#[test]
fn attr_guard_sweep_root_blocking_handles_missing_dir() {
    let tmp = tempdir();
    let nonexistent = tmp.as_path().join("does-not-exist");
    let (applied, _queued) = AttrGuard::sweep_root_blocking(&nonexistent);
    assert_eq!(applied, 0);
}

#[test]
fn attr_guard_sweep_root_blocking_walks_subdirs() {
    let tmp = tempdir();
    let root = tmp.as_path().join(".bentodesk");
    let zone_a = root.join("zone-a");
    let zone_b = root.join("zone-b");
    std::fs::create_dir_all(&zone_a).expect("zone-a");
    std::fs::create_dir_all(&zone_b).expect("zone-b");

    let (applied, _queued) = AttrGuard::sweep_root_blocking(&root);
    // root + zone-a + zone-b = 3 directories stamped.
    assert_eq!(applied, 3);
}

#[test]
fn cleanup_legacy_hidden_dir_removes_old_appdata_manifest() {
    let tmp = tempdir();
    let desktop = tmp.as_path().join("desktop");
    let app_data = tmp.as_path().join("appdata");
    std::fs::create_dir_all(&desktop).expect("desktop");
    std::fs::create_dir_all(&app_data).expect("appdata");
    let cfg = config_for(&desktop, &app_data);

    // Plant an old app-data manifest.json that should be deleted.
    let old_manifest = app_data.join("manifest.json");
    std::fs::write(&old_manifest, b"{}").expect("seed");

    cleanup_legacy_hidden_dir(&cfg, &[]).expect("cleanup");

    assert!(!old_manifest.exists(), "old manifest should be removed");
}
