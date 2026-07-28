#[test]
fn search_activation_for_action_queues_real_command() {
    let root = test_app_root();
    let _count = super::run_search_query(&root, "bulk");

    assert!(super::activate_search_hit(
        &root,
        "action:open_bulk_manager",
        std::ptr::null_mut()
    ));

    let mut commands: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let drained = root.dispatcher.drain_into(&mut commands);
    assert_eq!(drained, 1);
    assert!(matches!(commands.first(), Some(Command::OpenBulkManager)));
}

#[test]
fn search_activation_for_debug_overlay_action_queues_toggle_command() {
    let root = test_app_root();
    let _count = super::run_search_query(&root, "debug overlay");

    assert!(super::activate_search_hit(
        &root,
        "action:toggle_debug_overlay",
        std::ptr::null_mut()
    ));

    let mut commands: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let drained = root.dispatcher.drain_into(&mut commands);
    assert_eq!(drained, 1);
    assert!(matches!(
        commands.first(),
        Some(Command::ToggleDebugOverlay)
    ));
}

#[test]
fn search_activation_for_about_action_queues_open_about_command() {
    let root = test_app_root();
    let _count = super::run_search_query(&root, "open about");

    assert!(super::activate_search_hit(
        &root,
        "action:open_about",
        std::ptr::null_mut()
    ));

    let mut commands: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let drained = root.dispatcher.drain_into(&mut commands);
    assert_eq!(drained, 1);
    assert!(matches!(commands.first(), Some(Command::OpenAbout)));
}

#[test]
fn search_about_action_completes_in_one_dispatch_turn() {
    let root = test_app_root();
    let _count = super::run_search_query(&root, "open about");
    root.dispatcher.push(Command::ActivateSearchResult(
        smol_str::SmolStr::new_static("action:open_about"),
    ));

    super::consume_dispatcher(&root, std::ptr::null_mut());

    assert!(root.app.borrow().about_open.get());
    let mut commands: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    assert_eq!(root.dispatcher.drain_into(&mut commands), 0);
}

#[test]
fn search_query_indexes_list_minibars_action() {
    let root = test_app_root();

    let count = super::run_search_query(&root, "pinned minibars");

    assert!(count >= 1);
    let app = root.app.borrow();
    assert!(
        app.search_bar
            .borrow()
            .results
            .iter()
            .any(|hit| hit.id.as_str() == "action:list_minibars")
    );
}

#[test]
fn search_activation_for_list_minibars_action_queues_real_command() {
    let root = test_app_root();
    let _count = super::run_search_query(&root, "pinned minibars");

    assert!(super::activate_search_hit(
        &root,
        "action:list_minibars",
        std::ptr::null_mut()
    ));

    let mut commands: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let drained = root.dispatcher.drain_into(&mut commands);
    assert_eq!(drained, 1);
    assert!(matches!(
        commands.first(),
        Some(Command::ListPinnedMinibars)
    ));
}

#[test]
fn search_query_indexes_context_capsule_action() {
    let root = test_app_root();

    let count = super::run_search_query(&root, "context capsules");

    assert!(count >= 1);
    let app = root.app.borrow();
    assert!(
        app.search_bar
            .borrow()
            .results
            .iter()
            .any(|hit| hit.id.as_str() == "action:open_capsule_picker")
    );
}

#[test]
fn search_activation_for_context_capsule_action_queues_real_command() {
    let root = test_app_root();
    let _count = super::run_search_query(&root, "context capsules");

    assert!(super::activate_search_hit(
        &root,
        "action:open_capsule_picker",
        std::ptr::null_mut()
    ));

    let mut commands: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let drained = root.dispatcher.drain_into(&mut commands);
    assert_eq!(drained, 1);
    assert!(matches!(commands.first(), Some(Command::OpenCapsulePicker)));
}

#[test]
fn search_char_appends_query_and_queues_query_command() {
    let root = test_app_root();

    assert!(super::handle_search_char(
        &root,
        'A' as u32,
        std::ptr::null_mut()
    ));

    let app = root.app.borrow();
    assert_eq!(app.search_bar.borrow().query.as_str(), "A");
    drop(app);

    let mut commands: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let drained = root.dispatcher.drain_into(&mut commands);
    assert_eq!(drained, 1);
    assert!(matches!(
        commands.first(),
        Some(Command::QuerySearch(query)) if query.as_str() == "A"
    ));
}

#[test]
fn inline_zone_search_char_and_escape_animate_closed_on_main_surface() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(7), "Search", 20, 30, 240, 180));
        app.set_zone_display_mode(ZoneDisplayMode::Hover);
        app.zone_search_target.set(Some(ZoneId(7)));
    }

    assert!(super::handle_inline_zone_search_char(
        &root,
        'A' as u32,
        std::ptr::null_mut()
    ));
    assert_eq!(root.app.borrow().search_bar.borrow().query.as_str(), "A");

    assert_eq!(
        super::handle_inline_zone_search_keydown(&root, VK_ESCAPE_KEY, std::ptr::null_mut()),
        Some(0)
    );
    {
        let app = root.app.borrow();
        assert_eq!(app.zone_search_target.get(), Some(ZoneId(7)));
        assert!(app.zone_search_closing.get());
        assert_eq!(app.search_bar.borrow().query.as_str(), "A");
        let settled_at = unsafe {
            windows_sys::Win32::System::SystemInformation::GetTickCount().wrapping_add(1_000)
        };
        let _ = app.pill_animator.borrow_mut().tick(settled_at);
        assert!(settle_inline_zone_search_animation(&app, settled_at));
    }
    assert!(root.app.borrow().zone_search_target.get().is_none());
    assert!(root.app.borrow().search_bar.borrow().query.is_empty());
    assert_eq!(root.app.borrow().zone_pill_anim_zone.get(), Some(ZoneId(7)));
    assert!(!root.app.borrow().zone_pill_anim_expanding.get());
    assert_eq!(root.app.borrow().zone_pill_anim_from_morph.get(), 1.0);
}

#[test]
fn inline_zone_search_enter_opens_first_matching_item() {
    let root = test_app_root();
    let item_id = {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(77), "Inbox", 10, 20, 240, 180);
        let item_id = zone
            .add_item("C:/Desktop/Quarterly Report.pdf", "hash")
            .expect("item");
        app.zones.add(zone);
        app.zone_search_target.set(Some(ZoneId(77)));
        app.search_bar
            .borrow_mut()
            .set_query(SmolStr::new_static("report"));
        item_id
    };

    assert_eq!(
        super::handle_inline_zone_search_keydown(&root, VK_ENTER, std::ptr::null_mut()),
        Some(0)
    );
    let mut commands: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    assert_eq!(root.dispatcher.drain_into(&mut commands), 1);
    assert!(matches!(
        commands.first(),
        Some(Command::OpenItemFile(ZoneId(77), id)) if id.0 == item_id.0
    ));
}

#[test]
fn search_pointer_row_queues_activation_command() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 640.0,
            height: 520.0,
        };
        app.zones
            .add(Zone::new(ZoneId(77), "Contracts", 0, 0, 240, 160));
    }
    let _count = super::run_search_query(&root, "contracts");
    let row = bento_nano_app::business::search_bar::search_row_rect(root.app.borrow().viewport, 0);

    assert!(super::handle_search_lbutton_up(
        &root,
        std::ptr::null_mut(),
        row.x + 2.0,
        row.y + 2.0
    ));

    let mut commands: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let drained = root.dispatcher.drain_into(&mut commands);
    assert_eq!(drained, 1);
    assert!(matches!(
        commands.first(),
        Some(Command::ActivateSearchResult(hit_id)) if hit_id.as_str() == "zone:77"
    ));
}

#[test]
fn suggestor_seed_uses_real_grouping_backend_results() {
    let root = test_app_root();
    let files = smart_group_sample_files();

    let visible = seed_suggestor_from_files(&root, &files, 1, 0);

    assert!(visible >= 1);
    let app = root.app.borrow();
    let suggestor = app.suggestor.borrow();
    let first = suggestor.entries().first().expect("suggestion row");
    assert_eq!(first.suggestion.name, "Documents");
    assert_eq!(first.suggestion.matching_files.len(), 4);
    assert!(
        app.suggestor_status
            .borrow()
            .as_ref()
            .is_some_and(|status| status.as_str().contains("suggestion"))
    );
}

#[test]
fn suggestor_enter_dispatches_grouping_apply() {
    let root = test_app_root();
    let files = smart_group_sample_files();
    seed_suggestor_from_files(&root, &files, 1, 0);

    let _ = handle_suggestor_keydown(&root, VK_ENTER, std::ptr::null_mut());

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::GroupingApply { suggestion }) if suggestion.name == "Documents"
    ));
}

#[test]
fn suggestor_grouping_apply_updates_selected_zone_without_minting_overlap_card() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("suggestor-existing-target");
    let files = smart_group_sample_files();
    let suggestion = bento_nano_backend::grouping::suggest_groups(&files)
        .into_iter()
        .find(|entry| entry.name == "Documents")
        .expect("documents suggestion");

    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut target = Zone::new(ZoneId(42), "Inbox", 88, 66, 420, 300);
        let _ = target.add_item(
            Cow::Owned(suggestion.matching_files[0].clone()),
            Cow::Borrowed(""),
        );
        app.zones.add(target);
        app.selected_zone.set(Some(ZoneId(42)));
    }

    root.dispatcher.push(Command::GroupingApply {
        suggestion: Box::new(suggestion),
    });
    consume_dispatcher(&root, std::ptr::null_mut());

    {
        let app = root.app.borrow();
        assert_eq!(app.zones.len(), 1, "Apply must not mint a second Zone");
        let target = app.zones.get(ZoneId(42)).expect("selected target survives");
        assert_eq!(target.title.as_ref(), "Inbox");
        assert_eq!((target.x, target.y, target.w, target.h), (88, 66, 420, 300));
        assert_eq!(
            target.items.len(),
            4,
            "duplicate path is not appended twice"
        );
        assert!(
            !app.dirty.get(),
            "dispatcher must persist the applied items"
        );
    }
    let persisted = storage::read_zones(&zones_path).expect("persisted zones");
    assert_eq!(persisted.len(), 1);
    assert_eq!(
        persisted.get(ZoneId(42)).map(|zone| zone.items.len()),
        Some(4)
    );
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn suggestor_keyboard_manual_selection_filters_grouping_apply() {
    let root = test_app_root();
    let files = smart_group_sample_files();
    seed_suggestor_from_files(&root, &files, 1, 0);

    let _ = handle_suggestor_keydown(&root, VK_SPACE_KEY, std::ptr::null_mut());
    let _ = handle_suggestor_keydown(&root, VK_ENTER, std::ptr::null_mut());

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    match drained.first() {
        Some(Command::GroupingApply { suggestion }) => {
            assert_eq!(suggestion.name, "Documents");
            assert_eq!(suggestion.matching_files.len(), 3);
            assert!(
                !suggestion
                    .matching_files
                    .iter()
                    .any(|path| path.ends_with("doc0.pdf"))
            );
        }
        other => panic!("expected manually filtered GroupingApply, got {other:?}"),
    }
}

#[test]
fn suggestor_keyboard_none_blocks_empty_apply_then_all_restores() {
    let root = test_app_root();
    let files = smart_group_sample_files();
    seed_suggestor_from_files(&root, &files, 1, 0);

    let _ = handle_suggestor_keydown(&root, VK_N_KEY, std::ptr::null_mut());
    let _ = handle_suggestor_keydown(&root, VK_ENTER, std::ptr::null_mut());
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);

    let _ = handle_suggestor_keydown(&root, VK_A_KEY, std::ptr::null_mut());
    let _ = handle_suggestor_keydown(&root, VK_ENTER, std::ptr::null_mut());
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::GroupingApply { suggestion }) if suggestion.matching_files.len() == 4
    ));
}

#[test]
fn suggestor_dismiss_command_prunes_visible_row() {
    let root = test_app_root();
    let files = smart_group_sample_files();
    seed_suggestor_from_files(&root, &files, 1, 0);
    let before = root.app.borrow().suggestor.borrow().visible_count();

    let _ = handle_suggestor_keydown(&root, VK_D_KEY, std::ptr::null_mut());
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    assert_eq!(
        app.suggestor.borrow().visible_count(),
        before.saturating_sub(1)
    );
    assert_eq!(app.suggestor_dismissed.borrow().len(), 1);
}

#[test]
fn suggestor_selection_highlights_matching_zone_items() {
    let root = test_app_root();
    let files = smart_group_sample_files();
    {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(5), "Docs", 10, 20, 260, 180);
        for file in &files {
            let _item_id = zone.add_item(Cow::Owned(file.path.clone()), Cow::Borrowed("hash"));
        }
        app.zones.add(zone);
    }
    seed_suggestor_from_files(&root, &files, 1, 0);

    let highlighted = super::set_highlight_for_suggestor_selection(&root);

    assert_eq!(highlighted, 4);
    let app = root.app.borrow();
    assert_eq!(app.highlight_overlay.borrow().targets().len(), 4);
}

#[test]
fn suggestor_none_selection_clears_matching_highlight() {
    let root = test_app_root();
    let files = smart_group_sample_files();
    {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(5), "Docs", 10, 20, 260, 180);
        for file in &files {
            let _item_id = zone.add_item(Cow::Owned(file.path.clone()), Cow::Borrowed("hash"));
        }
        app.zones.add(zone);
    }
    seed_suggestor_from_files(&root, &files, 1, 0);
    let _highlighted = super::set_highlight_for_suggestor_selection(&root);
    let _ = handle_suggestor_keydown(&root, VK_N_KEY, std::ptr::null_mut());

    let app = root.app.borrow();
    assert!(!app.highlight_overlay.borrow().has_targets());
}

#[test]
fn suggestor_pointer_apply_dispatches_grouping_apply() {
    let root = test_app_root();
    let files = smart_group_sample_files();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 480.0,
            height: 360.0,
        };
    }
    seed_suggestor_from_files(&root, &files, 1, 0);
    let rect = smart_group_suggestor::suggestor_apply_rect(root.app.borrow().viewport, 0);

    assert!(handle_suggestor_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 2.0,
        rect.y + 2.0
    ));

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::GroupingApply { suggestion }) if suggestion.name == "Documents"
    ));
}

#[test]
fn suggestor_pointer_preview_checkbox_filters_grouping_apply() {
    let root = test_app_root();
    let files = smart_group_sample_files();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 640.0,
            height: 560.0,
        };
    }
    seed_suggestor_from_files(&root, &files, 1, 0);
    let preview = smart_group_suggestor::suggestor_preview_file_rect(root.app.borrow().viewport, 0);

    assert!(handle_suggestor_lbutton_up(
        &root,
        std::ptr::null_mut(),
        preview.x + 2.0,
        preview.y + 2.0
    ));
    let _ = handle_suggestor_keydown(&root, VK_ENTER, std::ptr::null_mut());

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::GroupingApply { suggestion })
            if suggestion.name == "Documents" && suggestion.matching_files.len() == 3
    ));
}

#[test]
fn context_capsule_capture_list_restore_delete_round_trip() {
    let zones_path = scratch_zones_path("round-trip");
    let mut zones = ZoneList::new();
    zones.add(Zone::new(ZoneId(1), "Focus", 10, 20, 300, 200));

    let entry = capture_context_capsule_for_path(&zones_path, &zones, "Focus Session")
        .expect("capture context capsule");
    assert!(entry.id.as_str().contains("Focus-Session"));

    let listed = list_context_capsules_for_path(&zones_path).expect("list context capsules");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, entry.id);
    assert_eq!(
        listed[0].name,
        smol_str::SmolStr::new_static("Focus Session")
    );

    let restored =
        restore_context_capsule_for_path(&zones_path, entry.id.as_str()).expect("restore");
    assert_eq!(restored.len(), 1);
    assert!(restored.get(ZoneId(1)).is_some());
    assert_eq!(
        restored.get(ZoneId(1)).map(|zone| zone.title.as_ref()),
        Some("Focus")
    );

    delete_context_capsule_for_path(&zones_path, entry.id.as_str()).expect("delete");
    assert!(
        list_context_capsules_for_path(&zones_path)
            .expect("list after delete")
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn context_capsule_envelope_preserves_zones_and_window_context() {
    let mut zones = ZoneList::new();
    zones.add(Zone::new(ZoneId(3), "Research", 30, 40, 320, 240));
    let windows = vec![ContextCapsuleWindow {
        title: "Draft.docx - Word".to_owned(),
        class_name: "OpusApp".to_owned(),
        process_name: "WINWORD.EXE".to_owned(),
        rect: (100, 120, 900, 720),
        is_maximized: false,
    }];

    let payload = encode_context_capsule_envelope(&zones, "Research Session", windows.clone())
        .expect("encode envelope");
    assert!(context_capsule_payload_is_json(&payload));
    let envelope = decode_context_capsule_envelope(&payload).expect("decode envelope");
    assert_eq!(envelope.name, "Research Session");
    assert_eq!(envelope.windows, windows);

    let restored = decode_context_capsule_zones(&payload).expect("decode zones");
    assert_eq!(
        restored.get(ZoneId(3)).map(|zone| zone.title.as_ref()),
        Some("Research")
    );
}

#[test]
fn context_capsule_window_match_requires_two_signals_and_prefers_nearby() {
    let captured = ContextCapsuleWindow {
        title: "Report - Notepad".to_owned(),
        class_name: "Notepad".to_owned(),
        process_name: "notepad.exe".to_owned(),
        rect: (500, 500, 900, 800),
        is_maximized: false,
    };
    let live = vec![
        LiveContextWindow {
            hwnd: 1usize as HWND,
            title: "Report - Notepad".to_owned(),
            class_name: "Chrome_WidgetWin_1".to_owned(),
            process_name: "chrome.exe".to_owned(),
            rect: (500, 500, 900, 800),
        },
        LiveContextWindow {
            hwnd: 2usize as HWND,
            title: "Untitled - Notepad".to_owned(),
            class_name: "Notepad".to_owned(),
            process_name: "notepad.exe".to_owned(),
            rect: (10, 10, 410, 310),
        },
        LiveContextWindow {
            hwnd: 3usize as HWND,
            title: "Report - Notepad".to_owned(),
            class_name: "Notepad".to_owned(),
            process_name: "notepad.exe".to_owned(),
            rect: (490, 490, 890, 790),
        },
    ];

    assert_eq!(match_context_window(&captured, &live), Some(3usize as HWND));
    assert_eq!(
        match_context_window(
            &ContextCapsuleWindow {
                class_name: "Other".to_owned(),
                process_name: "other.exe".to_owned(),
                ..captured
            },
            &live
        ),
        None
    );
}
