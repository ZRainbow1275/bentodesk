use super::*;

fn item(id: &str, name: &str, original: Option<&str>, hidden: Option<&str>) -> StealthItem {
    StealthItem {
        id: id.to_string(),
        name: name.to_string(),
        original_path: original.map(String::from),
        hidden_path: hidden.map(String::from),
        file_missing: false,
    }
}

fn touch_file(path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, b"content").expect("touch file");
}

// Hand-rolled tempdir (no `tempfile` workspace dep).
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
    let path =
        std::env::temp_dir().join(format!("bento-restore-{}-{}", std::process::id(), suffix));
    std::fs::create_dir_all(&path).expect("tempdir");
    TmpDir(path)
}

// ── restore_zone_items_with_dirs — spec G ──────────────────────

#[test]
fn restore_zone_items_skips_ambiguous_distractor_without_moving_files() {
    let tmp = tempdir();
    let desktop = tmp.as_path().join("desktop");
    let hidden_root = tmp.as_path().join(".bentodesk");
    let zone_hidden = hidden_root.join("z-test");
    std::fs::create_dir_all(&desktop).expect("desktop");
    std::fs::create_dir_all(&zone_hidden).expect("zone hidden");

    // Two homonyms scanned by the resolver's shallow walk.
    let desktop_homonym = desktop.join("report.pdf");
    let hidden_homonym = hidden_root.join("report.pdf");
    touch_file(&desktop_homonym);
    touch_file(&hidden_homonym);

    let ambiguous = item("ambig-1", "report.pdf", None, None);

    let recoverable_path = zone_hidden.join("notes.txt");
    touch_file(&recoverable_path);
    let recoverable_dest = desktop.join("notes.txt");
    let recoverable = item(
        "ok-1",
        "notes.txt",
        Some(&recoverable_dest.to_string_lossy()),
        Some(&recoverable_path.to_string_lossy()),
    );

    let report = restore_zone_items_with_dirs(&[ambiguous, recoverable], &desktop, &hidden_root);

    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].item_id, "ambig-1");
    assert_eq!(
        report.skipped[0].reason,
        RestoreSkippedReason::AmbiguousDisplayName
    );

    assert!(
        desktop_homonym.exists(),
        "desktop homonym must NOT be deleted"
    );
    assert!(hidden_homonym.exists(), "hidden homonym must NOT be moved");

    assert_eq!(report.restored, 1);
    assert!(recoverable_dest.exists());
    assert!(!recoverable_path.exists());
}

// ── reconcile_zone_items_with_dirs ─────────────────────────────

#[test]
fn reconcile_moves_real_desktop_files_into_zone_subfolder() {
    let tmp = tempdir();
    let desktop = tmp.as_path().join("Desktop");
    let hidden = desktop.join(".bentodesk");
    std::fs::create_dir_all(&desktop).expect("create desktop");

    let zone_id = "zone-1";
    let names = [
        "Steam.lnk",
        "Discord.lnk",
        "VSCode.lnk",
        "Brave.lnk",
        "OBS.lnk",
    ];

    let mut items: Vec<StealthItem> = names
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let original = desktop.join(name);
            touch_file(&original);
            let stale_hidden = hidden
                .join(zone_id)
                .join(name)
                .to_string_lossy()
                .into_owned();
            item(
                &format!("item-{idx}"),
                name,
                Some(&original.to_string_lossy()),
                Some(&stale_hidden),
            )
        })
        .collect();

    let report = reconcile_zone_items_with_dirs(&mut items, zone_id, &desktop, &hidden);

    assert_eq!(report.reconciled_count, 5);
    assert_eq!(report.already_managed_count, 0);
    assert_eq!(report.missing_count, 0);
    assert_eq!(report.unknown_count, 0);

    let zone_dir = hidden.join(zone_id);
    assert!(zone_dir.is_dir());

    for (idx, name) in names.iter().enumerate() {
        let original = desktop.join(name);
        assert!(!original.exists(), "{name} should have moved");
        let it = &items[idx];
        assert!(!it.file_missing);
        let new_hidden = it.hidden_path.as_deref().expect("hidden_path");
        assert!(Path::new(new_hidden).exists());
        assert!(Path::new(new_hidden).starts_with(&zone_dir));
    }
}

#[test]
fn reconcile_is_idempotent_after_first_pass() {
    let tmp = tempdir();
    let desktop = tmp.as_path().join("Desktop");
    let hidden = desktop.join(".bentodesk");
    std::fs::create_dir_all(&desktop).expect("create desktop");

    let zone_id = "zone-idem";
    let original = desktop.join("Notes.lnk");
    touch_file(&original);

    let mut items = vec![item(
        "i-1",
        "Notes.lnk",
        Some(&original.to_string_lossy()),
        None,
    )];

    let pass1 = reconcile_zone_items_with_dirs(&mut items, zone_id, &desktop, &hidden);
    assert_eq!(pass1.reconciled_count, 1);

    let pass2 = reconcile_zone_items_with_dirs(&mut items, zone_id, &desktop, &hidden);
    assert_eq!(pass2.reconciled_count, 0);
    assert_eq!(pass2.already_managed_count, 1);
    assert_eq!(pass2.missing_count, 0);
}

#[test]
fn reconcile_flags_items_with_no_resolvable_path_as_missing() {
    let tmp = tempdir();
    let desktop = tmp.as_path().join("Desktop");
    let hidden = desktop.join(".bentodesk");
    std::fs::create_dir_all(&desktop).expect("create desktop");

    let mut items = vec![item(
        "ghost",
        "ghost.lnk",
        Some(&desktop.join("ghost.lnk").to_string_lossy()),
        Some(&hidden.join("zone-x").join("ghost.lnk").to_string_lossy()),
    )];

    let report = reconcile_zone_items_with_dirs(&mut items, "zone-x", &desktop, &hidden);

    assert_eq!(report.reconciled_count, 0);
    assert_eq!(report.missing_count, 1);
    assert_eq!(report.unknown_count, 0);
    assert!(items[0].file_missing);
}

#[test]
fn reconcile_isolates_filenames_across_zones() {
    let tmp = tempdir();
    let desktop = tmp.as_path().join("Desktop");
    let hidden = desktop.join(".bentodesk");
    std::fs::create_dir_all(&desktop).expect("create desktop");

    let original = desktop.join("Settings.lnk");
    touch_file(&original);

    let mut zone_a_items = vec![item(
        "a-1",
        "Settings.lnk",
        Some(&original.to_string_lossy()),
        None,
    )];
    let report_a = reconcile_zone_items_with_dirs(&mut zone_a_items, "zone-a", &desktop, &hidden);
    assert_eq!(report_a.reconciled_count, 1);

    let mut zone_b_items = vec![item(
        "b-1",
        "Settings.lnk",
        Some(&original.to_string_lossy()),
        None,
    )];
    let report_b = reconcile_zone_items_with_dirs(&mut zone_b_items, "zone-b", &desktop, &hidden);
    assert_eq!(report_b.reconciled_count, 0);
    assert_eq!(report_b.missing_count, 1);
    assert!(zone_b_items[0].file_missing);

    assert!(hidden.join("zone-a").join("Settings.lnk").exists());
    assert!(!hidden.join("zone-b").join("Settings.lnk").exists());
}

// ── restore_file ────────────────────────────────────────────────

#[test]
fn restore_file_moves_back_to_original() {
    let tmp = tempdir();
    let hidden = tmp.as_path().join(".bentodesk").join("doc.txt");
    let original = tmp.as_path().join("desktop").join("doc.txt");
    touch_file(&hidden);

    restore_file(&original.to_string_lossy(), &hidden.to_string_lossy()).expect("restore");

    assert!(original.exists());
    assert!(!hidden.exists());
}

#[test]
fn restore_file_skips_when_destination_already_exists() {
    let tmp = tempdir();
    let hidden = tmp.as_path().join(".bentodesk").join("doc.txt");
    let original = tmp.as_path().join("desktop").join("doc.txt");
    touch_file(&hidden);
    touch_file(&original);

    restore_file(&original.to_string_lossy(), &hidden.to_string_lossy()).expect("restore");

    // Destination preserved; hidden source removed to avoid duplication.
    assert!(original.exists());
    assert!(!hidden.exists());
}

#[test]
fn restore_file_errors_when_source_missing() {
    let tmp = tempdir();
    let hidden = tmp.as_path().join("nonexistent.txt");
    let original = tmp.as_path().join("desktop").join("nonexistent.txt");
    let result = restore_file(&original.to_string_lossy(), &hidden.to_string_lossy());
    assert!(matches!(
        result,
        Err(StealthError::RestoreSourceMissing { .. })
    ));
}

// ── verify_references ──────────────────────────────────────────

#[test]
fn verify_references_reports_missing_hidden_files() {
    let tmp = tempdir();
    let present = tmp.as_path().join("present.txt");
    touch_file(&present);

    let items = vec![
        item(
            "ok",
            "present.txt",
            Some("/some/orig"),
            Some(&present.to_string_lossy()),
        ),
        item(
            "bad",
            "missing.txt",
            Some("/missing/orig"),
            Some("/non/existent.txt"),
        ),
    ];

    let missing = verify_references(&items);
    assert_eq!(missing, vec!["/missing/orig"]);
}
