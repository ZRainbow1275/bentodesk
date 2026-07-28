#[test]
fn normalize_icon_slug_rejects_unknown_or_emoji_payloads() {
    assert_eq!(normalize_icon_slug("folder-open").as_str(), "folder_open");
    assert_eq!(normalize_icon_slug("file").as_str(), "document");
    assert_eq!(
        normalize_icon_slug("not_a_real_icon").as_str(),
        DEFAULT_ZONE_ICON
    );
    assert_eq!(normalize_icon_slug("\u{1F4C1}").as_str(), DEFAULT_ZONE_ICON);
}

#[test]
fn icon_picker_pointer_commit_updates_zone_and_persists_restart_equivalent() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("icon-picker-pointer-persist");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones_path = zones_path.clone();
        app.zones
            .add(Zone::new(ZoneId(9), "Assets", 0, 0, 100, 100));
        app.icon_picker.borrow_mut().replace(IconPickerSession {
            zone_id: Some(ZoneId(9)),
            selected_icon: SmolStr::new_static("folder"),
        });
    }
    let slot = icon_picker_slot_rect(root.app.borrow().viewport, 21);

    assert!(handle_icon_picker_lbutton_up(
        &root,
        std::ptr::null_mut(),
        slot.x + (slot.width * 0.5),
        slot.y + (slot.height * 0.5)
    ));
    consume_dispatcher(&root, std::ptr::null_mut());

    {
        let app = root.app.borrow();
        assert_eq!(
            app.zones.get(ZoneId(9)).map(|zone| zone.icon.as_ref()),
            Some("folder_open")
        );
        assert!(!app.dirty.get());
    }
    let loaded = bentodesk_platform::storage::read_zones(&zones_path)
        .expect("zones.bin restart-equivalent load");
    assert_eq!(
        loaded.get(ZoneId(9)).map(|zone| zone.icon.as_ref()),
        Some("folder_open")
    );
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn zone_editor_uses_one_name_and_direct_appearance_controls() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 480.0,
            height: 460.0,
        };
        app.zone_editor.borrow_mut().replace(ZoneEditorSession {
            zone_id: ZoneId(6),
            draft_name: "Canonical".to_owned(),
            draft_icon: SmolStr::new_static("folder"),
            draft_accent_color: None,
            draft_grid_columns: 4,
            draft_capsule_size: SmolStr::new_static("large"),
            draft_capsule_shape: SmolStr::new_static("square"),
        });
    }
    let viewport = root.app.borrow().viewport;
    let name = zone_editor_name_input_rect(viewport);
    assert!(handle_zone_editor_lbutton_up(
        &root,
        std::ptr::null_mut(),
        name.x + 8.0,
        name.y + 8.0
    ));
    super::handle_zone_editor_char(&root, u32::from('!'));
    let accent = zone_editor_accent_option_rect(viewport, 3).expect("accent swatch");
    assert!(handle_zone_editor_lbutton_up(
        &root,
        std::ptr::null_mut(),
        accent.x + accent.width * 0.5,
        accent.y + accent.height * 0.5
    ));
    let columns = zone_editor_grid_option_rect(viewport, 6).expect("six-column segment");
    assert!(handle_zone_editor_lbutton_up(
        &root,
        std::ptr::null_mut(),
        columns.x + columns.width * 0.5,
        columns.y + columns.height * 0.5
    ));

    {
        let app = root.app.borrow();
        let editor = app.zone_editor.borrow();
        let editor = editor.as_ref().expect("editor remains open");
        assert_eq!(editor.draft_name, "Canonical!");
        assert_eq!(editor.draft_accent_color.as_deref(), Some("#22c55e"));
        assert_eq!(editor.draft_grid_columns, 6);
    }

    super::save_zone_editor(&root);
    let mut commands = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut commands), 6);
    assert!(commands.iter().any(|command| matches!(
        command,
        Command::SetZoneAlias(ZoneId(6), alias) if alias.is_empty()
    )));
    assert!(commands.iter().any(|command| matches!(
        command,
        Command::SetZoneAccent(ZoneId(6), Some(accent)) if accent.as_str() == "#22c55e"
    )));
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::SetZoneGridColumns(ZoneId(6), 6)))
    );
    assert!(commands.iter().any(|command| matches!(
        command,
        Command::SetZoneCapsule(ZoneId(6), size, shape)
            if size.as_str() == "large" && shape.as_str() == "square"
    )));
}

#[test]
fn zone_editor_icon_row_queues_icon_picker_for_same_zone() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 640.0,
            height: 420.0,
        };
        app.zones
            .add(Zone::new(ZoneId(3), "Edit Me", 0, 0, 100, 100));
        app.zone_editor.borrow_mut().replace(ZoneEditorSession {
            zone_id: ZoneId(3),
            draft_name: "Edit Me".to_owned(),
            draft_icon: SmolStr::new_static("folder"),
            draft_accent_color: None,
            draft_grid_columns: 4,
            draft_capsule_size: SmolStr::new_static("medium"),
            draft_capsule_shape: SmolStr::new_static("pill"),
        });
    }
    let icon = zone_editor_icon_rect(root.app.borrow().viewport);

    assert!(handle_zone_editor_lbutton_up(
        &root,
        std::ptr::null_mut(),
        icon.x + (icon.width * 0.5),
        icon.y + (icon.height * 0.5)
    ));

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::OpenIconPicker {
            zone_id: Some(ZoneId(3))
        })
    ));
    assert!(root.app.borrow().zone_editor.borrow().is_some());
}

#[test]
fn zone_editor_self_painted_close_discards_draft() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 480.0,
            height: 460.0,
        };
        app.zone_editor.borrow_mut().replace(ZoneEditorSession {
            zone_id: ZoneId(8),
            draft_name: "Unsaved".to_owned(),
            draft_icon: SmolStr::new_static("folder"),
            draft_accent_color: None,
            draft_grid_columns: 4,
            draft_capsule_size: SmolStr::new_static("medium"),
            draft_capsule_shape: SmolStr::new_static("rounded"),
        });
    }
    let close = zone_editor_close_rect(root.app.borrow().viewport);

    assert!(handle_zone_editor_lbutton_up(
        &root,
        std::ptr::null_mut(),
        close.x + close.width * 0.5,
        close.y + close.height * 0.5
    ));
    assert!(root.app.borrow().zone_editor.borrow().is_none());
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
}

#[test]
fn set_zone_icon_updates_open_zone_editor_draft() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones.add(Zone::new(ZoneId(4), "Draft", 0, 0, 100, 100));
        app.zone_editor.borrow_mut().replace(ZoneEditorSession {
            zone_id: ZoneId(4),
            draft_name: "Draft".to_owned(),
            draft_icon: SmolStr::new_static("folder"),
            draft_accent_color: None,
            draft_grid_columns: 4,
            draft_capsule_size: SmolStr::new_static("medium"),
            draft_capsule_shape: SmolStr::new_static("pill"),
        });
    }

    root.dispatcher.push(Command::SetZoneIcon(
        ZoneId(4),
        SmolStr::new_static("folder-open"),
    ));
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    assert_eq!(
        app.zones.get(ZoneId(4)).map(|zone| zone.icon.as_ref()),
        Some("folder_open")
    );
    assert_eq!(
        app.zone_editor
            .borrow()
            .as_ref()
            .map(|session| session.draft_icon.as_str()),
        Some("folder_open")
    );
}

#[test]
fn bulk_palette_picker_commit_mutates_selected_zone_accents() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
        app.zones.add(Zone::new(ZoneId(2), "Two", 0, 0, 100, 100));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
        app.bulk_manager.borrow_mut().toggle_selection(ZoneId(1));
        app.bulk_manager.borrow_mut().toggle_selection(ZoneId(2));
        app.palette_picker
            .borrow_mut()
            .replace(PalettePickerSession {
                target: PaletteTarget::BulkManagerSelectedAccent,
                selected_accent: Some(SmolStr::new_static("#22c55e")),
            });
    }

    save_palette_picker(&root);
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    assert_eq!(
        app.zones
            .get(ZoneId(1))
            .and_then(|zone| zone.accent_color.as_deref()),
        Some("#22c55e")
    );
    assert_eq!(
        app.zones
            .get(ZoneId(2))
            .and_then(|zone| zone.accent_color.as_deref()),
        Some("#22c55e")
    );
    assert!(app.dirty.get());
}

#[test]
fn bulk_palette_picker_clear_removes_selected_zone_accent() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(1), "One", 0, 0, 100, 100);
        zone.set_accent_color(Some(Cow::Borrowed("#3b82f6")));
        app.zones.add(zone);
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
        app.bulk_manager.borrow_mut().toggle_selection(ZoneId(1));
        app.palette_picker
            .borrow_mut()
            .replace(PalettePickerSession {
                target: PaletteTarget::BulkManagerSelectedAccent,
                selected_accent: None,
            });
    }

    save_palette_picker(&root);
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    assert!(
        app.zones
            .get(ZoneId(1))
            .and_then(|zone| zone.accent_color.as_deref())
            .is_none()
    );
    assert!(app.dirty.get());
}

#[test]
fn bulk_manager_text_button_starts_editor_and_enter_dispatches_update() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
        app.bulk_manager.borrow_mut().toggle_selection(ZoneId(1));
    }
    let text = bentodesk_app::business::bulk_manager_panel::BULK_MANAGER_ACTION_BUTTONS
        .iter()
        .find(|spec| {
            spec.hit
                == bentodesk_app::business::bulk_manager_panel::BulkManagerPointerHit::TextEdit
        })
        .copied()
        .expect("text edit button");
    let rect = bentodesk_app::business::bulk_manager_panel::bulk_manager_button_rect(
        root.app.borrow().viewport,
        text,
    );

    assert!(handle_bulk_manager_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 2.0,
        rect.y + 2.0
    ));
    assert_eq!(
        root.app
            .borrow()
            .bulk_manager
            .borrow()
            .text_edit()
            .map(|edit| edit.field),
        Some(BulkTextEditField::Alias)
    );
    assert!(handle_bulk_manager_char(&root, 'D' as u32));
    assert!(handle_bulk_manager_char(&root, 'o' as u32));
    assert!(handle_bulk_manager_char(&root, 'c' as u32));
    let _ = handle_bulk_manager_text_edit_keydown(&root, VK_ENTER, std::ptr::null_mut());

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::BulkUpdateZones(updates))
            if updates.len() == 1
                && updates[0].id == ZoneId(1)
                && updates[0].alias.as_deref() == Some("Doc")
    ));
    assert!(
        root.app
            .borrow()
            .bulk_manager
            .borrow()
            .text_edit()
            .is_none()
    );
}

#[test]
fn bulk_text_update_builder_encodes_alias_icon_accent_capsule_and_mode() {
    let alias = bulk_text_update_for_id(ZoneId(1), BulkTextEditField::Alias, "  Ops alias  ")
        .expect("alias update");
    assert_eq!(alias.alias.as_deref(), Some("Ops alias"));

    let icon = bulk_text_update_for_id(ZoneId(1), BulkTextEditField::Icon, " folder-open ")
        .expect("icon update");
    assert_eq!(icon.icon.as_deref(), Some("folder-open"));

    let accent = bulk_text_update_for_id(ZoneId(1), BulkTextEditField::Accent, "#ABC123")
        .expect("accent update");
    assert_eq!(accent.accent_color.as_deref(), Some("#ABC123"));

    let capsule = bulk_text_update_for_id(ZoneId(1), BulkTextEditField::CapsuleSize, " LARGE ")
        .expect("capsule update");
    assert_eq!(capsule.capsule_size.as_deref(), Some("large"));

    let mode = bulk_text_update_for_id(ZoneId(1), BulkTextEditField::DisplayMode, "clear")
        .expect("mode clear update");
    assert_eq!(mode.display_mode, Some(None));

    let mode_value = bulk_text_update_for_id(ZoneId(1), BulkTextEditField::DisplayMode, "Always")
        .expect("mode update");
    assert_eq!(
        mode_value.display_mode,
        Some(Some(SmolStr::new_static("always")))
    );
}

#[test]
fn bulk_text_update_builder_rejects_invalid_tokens() {
    assert!(bulk_text_update_for_id(ZoneId(1), BulkTextEditField::Icon, " ").is_err());
    assert!(bulk_text_update_for_id(ZoneId(1), BulkTextEditField::Accent, "blue").is_err());
    assert!(bulk_text_update_for_id(ZoneId(1), BulkTextEditField::CapsuleSize, "huge").is_err());
    assert!(
        bulk_text_update_for_id(ZoneId(1), BulkTextEditField::DisplayMode, "sometimes").is_err()
    );
}

#[test]
fn bulk_text_updates_for_selected_builds_one_payload_per_selected_zone() {
    let updates =
        bulk_text_updates_for_selected(&[ZoneId(1), ZoneId(2)], BulkTextEditField::Alias, "Shared")
            .expect("bulk text updates");
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0].id, ZoneId(1));
    assert_eq!(updates[1].id, ZoneId(2));
    assert_eq!(updates[0].alias.as_deref(), Some("Shared"));
    assert_eq!(updates[1].alias.as_deref(), Some("Shared"));
}

#[test]
fn bulk_metadata_cycle_helpers_match_known_tokens() {
    let zone = Zone::new(ZoneId(7), "Ops", 0, 0, 100, 100);
    assert_eq!(next_bulk_accent(None).as_str(), "#3b82f6");
    assert_eq!(next_bulk_capsule_size("small").as_str(), "medium");
    assert_eq!(next_bulk_capsule_size("medium").as_str(), "large");
    assert_eq!(next_bulk_capsule_size("large").as_str(), "small");
    assert_eq!(
        next_bulk_display_mode(None)
            .as_ref()
            .map(|value| value.as_str()),
        Some("hover")
    );
    assert_eq!(
        next_bulk_display_mode(Some("hover"))
            .as_ref()
            .map(|value| value.as_str()),
        Some("always")
    );
    assert_eq!(next_bulk_display_mode(Some("click")), None);
    assert_eq!(next_bulk_alias(&zone).as_str(), "Bulk Ops");
}

#[test]
fn bento_zone_conversion_preserves_bulk_metadata() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 800.0,
        height: 600.0,
    };
    let mut zone = Zone::new(ZoneId(5), "Canonical", 80, 60, 240, 180);
    zone.set_locked(true);
    zone.set_alias(Some(Cow::Borrowed("Alias")));
    zone.set_display_mode(Some(Cow::Borrowed("always")));
    app.zones.add(zone);

    let backend_zones = bento_zones_from_app(&app);
    assert!(backend_zones[0].locked);
    assert_eq!(backend_zones[0].alias.as_deref(), Some("Alias"));
    assert_eq!(
        backend_zones[0]
            .display_mode
            .as_ref()
            .map(|value| value.as_str()),
        Some("always")
    );

    let roundtrip = zone_list_from_bento_zones(&backend_zones, app.viewport);
    let zone = roundtrip.get(ZoneId(5)).expect("zone");
    assert!(zone.locked);
    assert_eq!(zone.alias.as_deref(), Some("Alias"));
    assert_eq!(zone.display_mode.as_deref(), Some("always"));
}

#[test]
fn clamp_zone_rect_pulls_high_percent_zone_fully_on_screen() {
    // ROOT-CAUSE-corrupt-zone-geometry.md Part 3: a zone at ~90% x with a
    // big body, migrated against a SMALL viewport (the 1707× 960 logical
    // screen), must end up fully on-screen (x + w <= vp_w, y + h <= vp_h).
    let vp = Size {
        width: 1707.0,
        height: 960.0,
    };
    // x near 90% of width, oversized body.
    let (x, y, w, h) = clamp_zone_rect_to_viewport(1536, 864, 800, 600, vp);
    assert!(x >= 0, "x >= 0");
    assert!(y >= 0, "y >= 0");
    assert!(x + w <= 1707, "x+w within viewport: {x}+{w}");
    assert!(y + h <= 960, "y+h within viewport: {y}+{h}");
    assert!(w >= super::MIN_MIGRATED_ZONE_DIMENSION, "w not collapsed");
    assert!(h >= super::MIN_MIGRATED_ZONE_DIMENSION, "h not collapsed");
}

#[test]
fn clamp_zone_rect_caps_oversized_body_to_viewport() {
    // A body far larger than the viewport (the 170667× 91200 corruption,
    // already neutralised by storage clamp but defended again here) is
    // capped to the viewport extents.
    let vp = Size {
        width: 1280.0,
        height: 720.0,
    };
    let (x, y, w, h) = clamp_zone_rect_to_viewport(0, 0, 170_667, 91_200, vp);
    assert_eq!((x, y), (0, 0));
    assert!(w <= 1280 && h <= 720, "body capped to viewport: {w}x{h}");
    assert!(x + w <= 1280 && y + h <= 720);
}

#[test]
fn clamp_zone_rect_keeps_in_bounds_zone_intact() {
    // A zone already comfortably inside the viewport passes through
    // unchanged.
    let vp = Size {
        width: 1920.0,
        height: 1080.0,
    };
    assert_eq!(
        clamp_zone_rect_to_viewport(100, 80, 400, 300, vp),
        (100, 80, 400, 300)
    );
}

#[test]
fn migrated_high_percent_zone_ends_up_on_screen() {
    // End-to-end through `zone_list_from_bento_zones`: a 90%-x zone on a
    // small viewport migrates fully on-screen.
    let vp = Size {
        width: 1707.0,
        height: 960.0,
    };
    let mut layout = legacy_layout_with_zone("90", "Edge", "Edge.txt");
    layout.zones[0].position = RelativePosition {
        x_percent: 90.0,
        y_percent: 85.0,
    };
    layout.zones[0].expanded_size = RelativeSize {
        w_percent: 40.0,
        h_percent: 40.0,
    };

    let zones = zone_list_from_bento_zones(&layout.zones, vp);
    let zone = zones.iter().next().expect("migrated zone");
    assert!(zone.x >= 0 && zone.y >= 0);
    assert!(
        zone.x + zone.w <= 1707,
        "x+w on-screen: {}+{}",
        zone.x,
        zone.w
    );
    assert!(
        zone.y + zone.h <= 960,
        "y+h on-screen: {}+{}",
        zone.y,
        zone.h
    );
}

#[test]
fn bulk_layout_algorithm_grid_moves_only_matched_zones() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 1000.0,
        height: 500.0,
    };
    app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
    app.zones.add(Zone::new(ZoneId(2), "Two", 0, 0, 100, 100));
    app.zones
        .add(Zone::new(ZoneId(3), "Untouched", 900, 400, 100, 100));

    let (changed, matched) = apply_bulk_layout_algorithm(
        &mut app,
        &[ZoneId(1), ZoneId(99), ZoneId(2)],
        BulkLayoutAlgorithm::Grid,
    );

    assert_eq!((changed, matched), (2, 2));
    let one = app.zones.get(ZoneId(1)).expect("zone 1");
    assert_eq!((one.x, one.y), (50, 25));
    let two = app.zones.get(ZoneId(2)).expect("zone 2");
    // The stale id keeps its deterministic grid slot, so Zone 2 remains in
    // column 0 of row 1. The row itself is footprint-aware and uses the full
    // safe work area rather than placing a 48-DIP capsule at a raw 50% origin.
    assert_eq!((two.x, two.y), (50, 427));
    let untouched = app.zones.get(ZoneId(3)).expect("zone 3");
    assert_eq!((untouched.x, untouched.y), (900, 400));
}

#[test]
fn bulk_layout_algorithm_skips_locked_zones_but_counts_match() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 1000.0,
        height: 500.0,
    };
    let mut locked = Zone::new(ZoneId(1), "Locked", 700, 300, 100, 100);
    locked.set_locked(true);
    app.zones.add(locked);
    app.zones.add(Zone::new(ZoneId(2), "Free", 0, 0, 100, 100));

    let (changed, matched) =
        apply_bulk_layout_algorithm(&mut app, &[ZoneId(1), ZoneId(2)], BulkLayoutAlgorithm::Row);

    assert_eq!((changed, matched), (1, 2));
    let locked = app.zones.get(ZoneId(1)).expect("locked");
    assert_eq!((locked.x, locked.y), (700, 300));
    let free = app.zones.get(ZoneId(2)).expect("free");
    assert_ne!((free.x, free.y), (0, 0));
}
