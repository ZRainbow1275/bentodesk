#[test]
fn tray_callback_maps_legacy_and_v4_events_to_commands() {
    assert!(matches!(
        tray_command_for_callback(0, WM_LBUTTONUP as _),
        Some(Command::ShowWindow(WindowKind::Main))
    ));
    assert!(matches!(
        tray_command_for_callback(0, NIN_SELECT as _),
        Some(Command::ShowWindow(WindowKind::Main))
    ));
    assert!(matches!(
        tray_command_for_callback(0, WM_RBUTTONUP as _),
        Some(Command::ShowTrayMenu)
    ));
    assert!(matches!(
        tray_command_for_callback(0, WM_CONTEXTMENU as _),
        Some(Command::ShowTrayMenu)
    ));
    assert!(tray_command_for_callback(0, 0).is_none());
}

#[test]
fn item_context_action_mapping_covers_d2d_menu_ids() {
    let move_targets = vec![
        (
            ITEM_CONTEXT_MOVE_ZONE_BASE_ID,
            ZoneId(12),
            SmolStr::new_static("Work"),
        ),
        (
            ITEM_CONTEXT_MOVE_ZONE_BASE_ID + 1,
            ZoneId(13),
            SmolStr::new_static("Later"),
        ),
    ];

    assert_eq!(
        item_context_action_for_choice(ITEM_CONTEXT_OPEN_ID, &move_targets),
        Some(ItemContextAction::Open)
    );
    assert_eq!(
        item_context_action_for_choice(ITEM_CONTEXT_REVEAL_ID, &move_targets),
        Some(ItemContextAction::Reveal)
    );
    assert_eq!(
        item_context_action_for_choice(ITEM_CONTEXT_COPY_PATH_ID, &move_targets),
        Some(ItemContextAction::CopyPath)
    );
    assert_eq!(
        item_context_action_for_choice(ITEM_CONTEXT_RENAME_FILE_ID, &move_targets),
        Some(ItemContextAction::RenameFile)
    );
    assert_eq!(
        item_context_action_for_choice(ITEM_CONTEXT_DELETE_FILE_ID, &move_targets),
        Some(ItemContextAction::DeleteFile)
    );
    assert_eq!(
        item_context_action_for_choice(ITEM_CONTEXT_TOGGLE_WIDE_ID, &move_targets),
        Some(ItemContextAction::ToggleWide)
    );
    assert_eq!(
        item_context_action_for_choice(ITEM_CONTEXT_REMOVE_ID, &move_targets),
        Some(ItemContextAction::Remove)
    );
    assert_eq!(
        item_context_action_for_choice(ITEM_CONTEXT_MOVE_ZONE_BASE_ID + 1, &move_targets),
        Some(ItemContextAction::MoveToZone(ZoneId(13)))
    );
    assert_eq!(
        item_context_action_for_choice(ITEM_CONTEXT_MOVE_ZONE_BASE_ID + 64, &move_targets),
        None
    );
    assert_eq!(item_context_action_for_choice(0, &move_targets), None);
}
#[test]
fn d2d_zone_menu_keeps_dynamic_targets_in_one_submenu() {
    let entries = zone_context_menu_rows(false, false, true);
    let command_rows = entries
        .iter()
        .filter(|entry| entry.kind == popover::ContextMenuRowKind::Command)
        .count();
    assert!(command_rows <= 12, "main context menu must stay compact");
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.kind == popover::ContextMenuRowKind::Submenu)
            .count(),
        1
    );
    assert_eq!(
        entries.last().map(|entry| entry.command_id),
        Some(super::ZONE_CONTEXT_DELETE_ID)
    );
    assert!(entries.last().is_some_and(|entry| entry.danger));
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.command_id == super::ZONE_CONTEXT_EDIT_ID)
            .count(),
        1,
        "one editor owns the single user-facing Zone name"
    );
    assert!(
        !entries
            .iter()
            .any(|entry| entry.command_id == super::ZONE_CONTEXT_UNSTACK_ID)
    );

    let stacked = zone_context_menu_rows(true, true, true);
    assert!(
        stacked
            .iter()
            .any(|entry| entry.command_id == super::ZONE_CONTEXT_UNSTACK_ID)
    );
    assert!(
        stacked
            .iter()
            .any(|entry| entry.command_id == super::ZONE_CONTEXT_OPEN_STACK_TRAY_ID)
    );
}

#[test]
fn d2d_context_menu_keyboard_navigation_skips_separators() {
    let rows = zone_context_menu_rows(false, false, false);
    let separator = rows
        .iter()
        .position(|row| row.kind == popover::ContextMenuRowKind::Separator)
        .expect("zone menu contains a separator");
    let session = popover::ContextMenuSession::new(rows, popover::ContextMenuRows::new());
    assert_eq!(
        context_menu_next_hit(
            &session,
            popover::ContextMenuColumn::Main,
            Some(separator - 1),
            true,
        ),
        Some(popover::ContextMenuHit {
            column: popover::ContextMenuColumn::Main,
            row: separator + 1,
        })
    );
    assert_eq!(
        context_menu_next_hit(
            &session,
            popover::ContextMenuColumn::Main,
            Some(separator + 1),
            false,
        ),
        Some(popover::ContextMenuHit {
            column: popover::ContextMenuColumn::Main,
            row: separator - 1,
        })
    );
}

#[test]
fn d2d_item_menu_separates_file_delete_and_collapses_move_targets() {
    let entries = item_context_menu_rows(true);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.kind == popover::ContextMenuRowKind::Submenu)
            .count(),
        1
    );
    let last = entries.last().expect("destructive row");
    assert_eq!(last.command_id, ITEM_CONTEXT_DELETE_FILE_ID);
    assert!(last.danger);
    assert!(
        entries
            .iter()
            .any(|entry| entry.command_id == ITEM_CONTEXT_REMOVE_ID)
    );
}

#[test]
fn item_context_dispatch_maps_actions_to_commands_and_side_effects() {
    let zone_id = ZoneId(7);
    let item_id = ZoneItemId(9);
    let path = r"C:\Users\Alice\Desktop\report.pdf";

    assert_eq!(
        item_context_dispatch_for_action(zone_id, item_id, path, ItemContextAction::Open),
        ItemContextDispatch::OpenPath {
            verb: "open",
            path: SmolStr::new(path),
        }
    );
    assert_eq!(
        item_context_dispatch_for_action(zone_id, item_id, path, ItemContextAction::Reveal),
        ItemContextDispatch::RevealPath(SmolStr::new(path))
    );
    assert_eq!(
        item_context_dispatch_for_action(zone_id, item_id, path, ItemContextAction::CopyPath),
        ItemContextDispatch::Command(Command::CopyItemPath(bentodesk_app::ItemPath::new(path)))
    );
    assert_eq!(
        item_context_dispatch_for_action(zone_id, item_id, path, ItemContextAction::RenameFile,),
        ItemContextDispatch::Command(Command::OpenItemFileRename(
            zone_id,
            bentodesk_app::ItemId(item_id.0),
        ))
    );
    assert_eq!(
        item_context_dispatch_for_action(zone_id, item_id, path, ItemContextAction::DeleteFile,),
        ItemContextDispatch::Command(Command::DeleteItemFileToRecycleBin(
            zone_id,
            bentodesk_app::ItemId(item_id.0),
        ))
    );
    assert_eq!(
        item_context_dispatch_for_action(zone_id, item_id, path, ItemContextAction::ToggleWide,),
        ItemContextDispatch::Command(Command::ToggleItemWide(
            zone_id,
            bentodesk_app::ItemId(item_id.0),
        ))
    );
    assert_eq!(
        item_context_dispatch_for_action(
            zone_id,
            item_id,
            path,
            ItemContextAction::MoveToZone(ZoneId(11)),
        ),
        ItemContextDispatch::Command(Command::MoveItemToZone(
            zone_id,
            ZoneId(11),
            bentodesk_app::ItemId(item_id.0),
        ))
    );
    assert_eq!(
        item_context_dispatch_for_action(zone_id, item_id, path, ItemContextAction::Remove),
        ItemContextDispatch::Command(Command::RemoveItem(
            zone_id,
            bentodesk_app::ItemId(item_id.0),
        ))
    );
}

#[test]
fn item_context_open_sets_visible_success_status() {
    let root = test_app_root();
    let path = r"C:\Users\Alice\Desktop\report.pdf";
    let mut captured: Option<(String, String, Option<String>)> = None;

    apply_item_context_dispatch_with(
        &root,
        ItemContextDispatch::OpenPath {
            verb: "open",
            path: SmolStr::new(path),
        },
        |verb, file, parameters| {
            captured = Some((
                verb.to_owned(),
                file.to_owned(),
                parameters.map(str::to_owned),
            ));
            Ok(())
        },
        |_| unreachable!("reveal should not run for open dispatch"),
    );

    assert_eq!(captured, Some(("open".to_owned(), path.to_owned(), None)));
    assert_eq!(
        root.app
            .borrow()
            .item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Open requested: report.pdf")
    );
}

#[test]
fn item_context_reveal_sets_visible_failure_status() {
    let root = test_app_root();
    let path = r"C:\Users\Alice\Desktop\missing.pdf";
    let mut captured: Option<String> = None;

    apply_item_context_dispatch_with(
        &root,
        ItemContextDispatch::RevealPath(SmolStr::new(path)),
        |_, _, _| unreachable!("open should not run for reveal dispatch"),
        |file| {
            captured = Some(file.to_owned());
            Err(31)
        },
    );

    assert_eq!(captured, Some(path.to_owned()));
    assert_eq!(
        root.app
            .borrow()
            .item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Reveal failed for missing.pdf: ShellExecuteW failed: 31")
    );
}

#[test]
fn item_context_command_dispatch_still_queues_dispatcher_command() {
    let root = test_app_root();
    let path = r"C:\Users\Alice\Desktop\report.pdf";

    apply_item_context_dispatch_with(
        &root,
        ItemContextDispatch::Command(Command::CopyItemPath(bentodesk_app::ItemPath::new(path))),
        |_, _, _| unreachable!("open should not run for command dispatch"),
        |_| unreachable!("reveal should not run for command dispatch"),
    );

    let mut pending = smallvec::SmallVec::<[Command; 8]>::new();
    root.dispatcher.drain_into(&mut pending);
    assert_eq!(
        pending.as_slice(),
        &[Command::CopyItemPath(bentodesk_app::ItemPath::new(path))]
    );
    assert!(
        root.app.borrow().item_operation_status.borrow().is_none(),
        "pure command dispatch must not invent shell-launch status"
    );
}

#[test]
fn copy_item_path_reports_visible_clipboard_success_and_failure() {
    let root = test_app_root();
    let path = r"C:\Users\Alice\Desktop\report.pdf";
    let mut captured_owner: Option<HWND> = None;
    let mut captured_text: Option<String> = None;

    assert!(copy_item_path_with(&root, path, |owner, text| {
        captured_owner = Some(owner);
        captured_text = Some(text.to_owned());
        true
    }));
    assert_eq!(captured_owner, Some(std::ptr::null_mut()));
    assert_eq!(captured_text, Some(path.to_owned()));
    assert_eq!(
        root.app
            .borrow()
            .item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Copied path: report.pdf")
    );

    assert!(!copy_item_path_with(&root, path, |_, text| {
        captured_text = Some(text.to_owned());
        false
    }));
    assert_eq!(captured_text, Some(path.to_owned()));
    assert_eq!(
        root.app
            .borrow()
            .item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Copy path failed: report.pdf")
    );
}

#[test]
fn add_item_accepts_real_file_under_custom_desktop_and_persists() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-add");
    let state_dir = zones_path.parent().expect("scratch parent");
    let desktop_dir = state_dir.join("Desktop");
    std::fs::create_dir_all(&desktop_dir).expect("desktop dir");
    let source = desktop_dir.join("new-item.txt");
    std::fs::write(&source, b"new").expect("source file");
    let desktop_source = desktop_dir.to_string_lossy().to_string();
    let source_path = source.to_string_lossy().to_string();
    let zone_id = ZoneId(44);
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.zones.add(Zone::new(zone_id, "Inbox", 0, 0, 240, 160));
    }

    assert!(add_item_to_zone_with(
        &root,
        zone_id,
        source_path.as_str(),
        Some(desktop_source.as_str()),
        |_| Some("hash-add".to_owned()),
    ));

    {
        let app = root.app.borrow();
        assert!(app.dirty.get());
        let item = app
            .zones
            .get(zone_id)
            .expect("zone")
            .items
            .first()
            .expect("added item");
        assert_ne!(item.path.as_ref(), source_path.as_str());
        assert_eq!(item.original_path.as_deref(), Some(source_path.as_str()));
        assert_eq!(item.hidden_path.as_deref(), Some(item.path.as_ref()));
        assert!(std::path::Path::new(item.path.as_ref()).exists());
        assert!(!source.exists(), "stealth-enabled add should hide source");
        assert_eq!(item.icon_hash.as_ref(), "hash-add");
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Added item: new-item.txt")
        );
        storage::write_zones_atomic(&zones_path, &app.zones).expect("persist zones");
    }
    let reloaded = storage::read_zones(&zones_path).expect("read persisted zones");
    assert_eq!(
        reloaded
            .get(zone_id)
            .expect("persisted zone")
            .items
            .first()
            .expect("persisted item")
            .original_path
            .as_deref(),
        Some(source_path.as_str())
    );

    let _ = std::fs::remove_dir_all(state_dir);
}

/// α2 (Wave I-α, 2026-05-25) — full WM_DROPFILES path E2E.
///
/// The R3 hand-test (handtest-quad-2026-05-25-0106/r3-summary.txt)
/// reported `TARGET_STILL_ON_DESKTOP=True` because the driver could not
/// synthesise a real OLE drag-drop from Explorer (the pill HWND has
/// `WS_EX_TRANSPARENT` so SetCursorPos+mouse_event events fall through
/// before reaching the BentoDesk window). This test bypasses the input
/// path entirely and exercises the receive side: WM_DROPFILES's terminal
/// helper `queue_add_items` enqueues `Command::AddItem` per file, which
/// the dispatcher then resolves through `add_item_to_zone_with` →    /// `hide_item_file` → `bentodesk_backend::stealth::hide_file`. If any
/// link in that chain breaks (mis-routed Command, dropped enqueue,
/// stealth path swallowed), this test fails — proving that what the
/// programmatic R3 hand-test can't see is in fact functional.
#[test]
fn alpha2_wm_dropfiles_chain_runs_stealth_hide_for_each_dropped_file() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("alpha2-dropfiles-stealth");
    let state_dir = zones_path.parent().expect("scratch parent");
    let desktop_dir = state_dir.join("Desktop");
    std::fs::create_dir_all(&desktop_dir).expect("desktop dir");
    let mut source_paths: Vec<String> = Vec::new();
    for name in ["alpha2-a.txt", "alpha2-b.txt"] {
        let p = desktop_dir.join(name);
        std::fs::write(&p, b"alpha2").expect("source file");
        source_paths.push(p.to_string_lossy().to_string());
    }
    let desktop_source = desktop_dir.to_string_lossy().to_string();
    let zone_id = ZoneId(7474);
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.zones.add(Zone::new(zone_id, "Drop", 0, 0, 240, 160));
    }

    // Drive the same entry point WM_DROPFILES uses — `queue_add_items`
    // pushes one Command::AddItem per file onto the dispatcher.
    queue_add_items(&root, zone_id, source_paths.clone(), "test::alpha2");

    // Drain the dispatcher manually (the test harness has no message
    // pump). Each AddItem command flows through the production
    // dispatcher arm, which calls `add_item_to_zone(...)`. The
    // test uses `add_item_to_zone_with(..., Some(desktop_source), ...)`
    // so the scratch desktop is recognised by the
    // `desktop_sources::is_under_any_desktop` guard.
    let mut queued: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let drained = root.dispatcher.drain_into(&mut queued);
    assert_eq!(drained, 2, "two AddItem commands must be queued");
    for cmd in queued.drain(..) {
        match cmd {
            Command::AddItem(z, p) => {
                assert_eq!(z, zone_id);
                assert!(add_item_to_zone_with(
                    &root,
                    z,
                    p.0.as_str(),
                    Some(desktop_source.as_str()),
                    |_| Some("hash-alpha2".to_owned()),
                ));
            }
            other => panic!("unexpected queued command: {other:?}"),
        }
    }

    // After the chain runs, both source files must be moved off the
    // desktop into the stealth manifest path; the zone must report two
    // items, each with an `original_path` matching the input and a
    // distinct `hidden_path` that points at an existing file.
    for source_path in &source_paths {
        let on_desktop = std::path::Path::new(source_path).exists();
        assert!(
            !on_desktop,
            "α2 chain broken: source {source_path} still on desktop \
                 after Command::AddItem dispatch — stealth_hide did not \
                 run or wrote back the original path"
        );
    }
    let app = root.app.borrow();
    let zone = app.zones.get(zone_id).expect("zone");
    assert_eq!(zone.items.len(), 2);
    let mut originals: Vec<String> = zone
        .items
        .iter()
        .map(|i| i.original_path.clone().unwrap_or_default().to_string())
        .collect();
    originals.sort();
    let mut expected = source_paths.clone();
    expected.sort();
    assert_eq!(originals, expected);
    for item in &zone.items {
        let hidden = item
            .hidden_path
            .as_deref()
            .expect("α2: every persisted item must carry a hidden_path");
        assert!(
            std::path::Path::new(hidden).exists(),
            "α2: hidden file {hidden} should exist after stealth-hide ran"
        );
        assert_ne!(
            item.path.as_ref(),
            item.original_path.as_deref().unwrap_or(""),
            "α2: persisted effective path must differ from original \
                 path (the original is on the desktop, the effective lives \
                 under .bentodesk/)"
        );
    }
    drop(app);

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn add_item_rejects_missing_and_outside_desktop_with_visible_status() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-add-reject");
    let state_dir = zones_path.parent().expect("scratch parent");
    let desktop_dir = state_dir.join("Desktop");
    std::fs::create_dir_all(&desktop_dir).expect("desktop dir");
    let outside = state_dir.join("outside.txt");
    std::fs::write(&outside, b"outside").expect("outside file");
    let desktop_source = desktop_dir.to_string_lossy().to_string();
    let missing = desktop_dir.join("missing.txt");
    let zone_id = ZoneId(45);
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.zones.add(Zone::new(zone_id, "Inbox", 0, 0, 240, 160));
    }

    assert!(add_item_to_zone_with(
        &root,
        zone_id,
        missing.to_string_lossy().as_ref(),
        Some(desktop_source.as_str()),
        |_| Some("unused".to_owned()),
    ));
    {
        let app = root.app.borrow();
        assert!(!app.dirty.get());
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Add item failed: missing missing.txt")
        );
    }

    assert!(add_item_to_zone_with(
        &root,
        zone_id,
        outside.to_string_lossy().as_ref(),
        Some(desktop_source.as_str()),
        |_| Some("unused".to_owned()),
    ));
    {
        let app = root.app.borrow();
        assert!(!app.dirty.get());
        assert!(app.zones.get(zone_id).expect("zone").items.is_empty());
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Add item rejected outside Desktop: outside.txt")
        );
    }

    let _ = std::fs::remove_dir_all(state_dir);
}
