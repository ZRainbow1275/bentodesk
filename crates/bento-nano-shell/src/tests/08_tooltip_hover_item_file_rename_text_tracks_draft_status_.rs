#[test]
fn tooltip_hover_item_file_rename_text_tracks_draft_status_and_path() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 472.0,
            height: 206.0,
        };
        app.item_file_rename
            .borrow_mut()
            .replace(ItemFileRenameSession {
                zone_id: ZoneId(7),
                item_id: ZoneItemId(9),
                draft_name: "report-final.pdf".to_owned(),
                current_path: SmolStr::new_static("C:/Users/BentoDeskTest/Desktop/report.pdf"),
                status: Some(SmolStr::new_static("ready")),
            });
    }
    let viewport = Size {
        width: 472.0,
        height: 206.0,
    };

    let path = item_file_rename_path_rect(viewport);
    let path_tooltip = {
        let app = root.app.borrow();
        tooltip_command_for_item_file_rename_hover(
            &app,
            bento_nano_app::WindowHandle::NULL,
            path.x + 8.0,
            path.y + 8.0,
        )
    };
    match path_tooltip {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Current file C:/Users/BentoDeskTest/Desktop/report.pdf");
        }
        other => panic!("expected item rename path tooltip, got {other:?}"),
    }

    let input = item_file_rename_input_rect(viewport);
    let input_tooltip = {
        let app = root.app.borrow();
        tooltip_command_for_item_file_rename_hover(
            &app,
            bento_nano_app::WindowHandle::NULL,
            input.x + 8.0,
            input.y + 8.0,
        )
    };
    match input_tooltip {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Rename to report-final.pdf");
        }
        other => panic!("expected item rename input tooltip, got {other:?}"),
    }

    let status = item_file_rename_status_rect(viewport);
    let status_tooltip = {
        let app = root.app.borrow();
        tooltip_command_for_item_file_rename_hover(
            &app,
            bento_nano_app::WindowHandle::NULL,
            status.x + 8.0,
            status.y + 8.0,
        )
    };
    match status_tooltip {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Rename validation: ready");
        }
        other => panic!("expected item rename status tooltip, got {other:?}"),
    }
}

#[test]
fn minibar_pointer_unpin_maps_to_real_zone_command() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.upsert_minibar(
            ZoneId(42),
            MiniBar::new(ui::HIDE_PATH, smol_str::SmolStr::new_static("Docs"), 43),
        );
    }
    let viewport = bento_nano_style::Size {
        width: 280.0,
        height: 80.0,
    };

    let command = {
        let app = root.app.borrow();
        minibar_command_for_pointer(&app, viewport, 250.0, 40.0)
    };
    assert!(matches!(command, Some(Command::UnpinMinibar(ZoneId(42)))));

    let body_command = {
        let app = root.app.borrow();
        minibar_command_for_pointer(&app, viewport, 50.0, 40.0)
    };
    assert!(body_command.is_none());
}

#[test]
fn minibar_pointer_item_maps_to_open_item_command() {
    let root = test_app_root();
    let mut zone = Zone::new(ZoneId(42), "Docs", 0, 0, 240, 160);
    let item_id = zone
        .add_item(
            Cow::Owned("C:/Desktop/contract.pdf".to_owned()),
            Cow::Borrowed("hash"),
        )
        .expect("item id");
    root.app.borrow_mut().zones.add(zone);
    assert!(pin_zone_minibar_state(&root, ZoneId(42)).is_some());
    let viewport = bento_nano_style::Size {
        width: 280.0,
        height: 80.0,
    };
    let bar = root
        .app
        .borrow()
        .active_minibar()
        .map(|(_, bar)| bar)
        .expect("active minibar");
    let item_rect =
        bento_nano_app::business::minibar::minibar_item_rect(viewport, &bar, 0).expect("item rect");

    let command = {
        let app = root.app.borrow();
        minibar_command_for_pointer(
            &app,
            viewport,
            item_rect.x + item_rect.width * 0.5,
            item_rect.y + item_rect.height * 0.5,
        )
    };

    assert!(matches!(
        command,
        Some(Command::OpenItemFile(ZoneId(42), id)) if id.0 == item_id.0
    ));
}

#[test]
fn open_item_file_from_zone_uses_real_item_path_and_visible_status() {
    let root = test_app_root();
    let mut zone = Zone::new(ZoneId(42), "Docs", 0, 0, 240, 160);
    let item_id = zone
        .add_item_with_metadata(
            Cow::Owned(
                "C:/Users/BentoDeskTest/AppData/Roaming/BentoDesk/.bentodesk/42/contract.pdf".to_owned(),
            ),
            Some("C:/Desktop/contract.pdf"),
            Cow::Borrowed("hash"),
            Some(Cow::Owned("C:/Desktop/contract.pdf".to_owned())),
            Some(Cow::Owned(
                "C:/Users/BentoDeskTest/AppData/Roaming/BentoDesk/.bentodesk/42/contract.pdf".to_owned(),
            )),
        )
        .expect("item id");
    root.app.borrow_mut().zones.add(zone);
    let opened = std::cell::RefCell::new(Vec::<String>::new());

    assert!(super::open_item_file_from_zone_with(
        &root,
        ZoneId(42),
        bento_nano_app::ItemId(item_id.0),
        |verb, path, params| {
            assert_eq!(verb, "open");
            assert!(params.is_none());
            opened.borrow_mut().push(path.to_owned());
            Ok(())
        }
    ));

    assert_eq!(
        opened.borrow().as_slice(),
        &["C:/Users/BentoDeskTest/AppData/Roaming/BentoDesk/.bentodesk/42/contract.pdf"]
    );
    let app = root.app.borrow();
    let status = app.item_operation_status.borrow();
    assert_eq!(
        status.as_ref().map(SmolStr::as_str),
        Some("Open requested: contract.pdf")
    );
}

#[test]
fn minibar_pins_round_trip_through_stable_csv_wire_value() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.upsert_minibar(
            ZoneId(42),
            MiniBar::new(ui::HIDE_PATH, smol_str::SmolStr::new_static("Docs"), 43),
        );
        app.upsert_minibar(
            ZoneId(7),
            MiniBar::new(ui::HIDE_PATH, smol_str::SmolStr::new_static("Projects"), 8),
        );
    }

    let csv = current_minibar_pins_csv(&root).expect("csv");
    assert_eq!(csv.as_str(), "42,7");
    let ids = parse_minibar_pin_ids(csv.as_str());
    assert_eq!(ids.as_slice(), &[ZoneId(42), ZoneId(7)]);
}

#[test]
fn minibar_pin_restore_parser_skips_invalid_duplicates_and_overflow() {
    let ids = parse_minibar_pin_ids(" 1,invalid,0,2,1,3,4,5,6,7,8,9,10 ");
    assert_eq!(
        ids.as_slice(),
        &[
            ZoneId(1),
            ZoneId(2),
            ZoneId(3),
            ZoneId(4),
            ZoneId(5),
            ZoneId(6),
            ZoneId(7),
            ZoneId(8),
        ]
    );
}

#[test]
fn minibar_restore_wire_value_pins_existing_zones_only() {
    let root = test_app_root();
    seed_test_zone(&root, 1, "Docs");
    seed_test_zone(&root, 2, "Projects");

    let restored =
        restore_minibar_pins_from_wire_value(&root, "1,missing,99,2,1", |root, zone_id| {
            pin_zone_minibar_state(root, zone_id).is_some()
        });

    assert_eq!(restored, 2);
    let csv = current_minibar_pins_csv(&root).expect("csv");
    assert_eq!(csv.as_str(), "1,2");
    assert_eq!(root.minibar_roster.borrow().len(), 2);
    assert_eq!(root.minibars.borrow().len(), 2);
    {
        let app = root.app.borrow();
        let minibars = app.minibars.borrow();
        assert_eq!(minibars[0].1.label.as_str(), "Docs");
        assert_eq!(minibars[1].1.label.as_str(), "Projects");
    }
}

#[test]
fn minibar_pin_state_is_idempotent_for_existing_zone() {
    let root = test_app_root();
    seed_test_zone(&root, 1, "Docs");

    assert!(pin_zone_minibar_state(&root, ZoneId(1)).is_some());
    assert!(pin_zone_minibar_state(&root, ZoneId(1)).is_some());

    let csv = current_minibar_pins_csv(&root).expect("csv");
    assert_eq!(csv.as_str(), "1");
    assert_eq!(root.minibar_roster.borrow().len(), 1);
    assert_eq!(root.minibars.borrow().len(), 1);
    assert_eq!(root.app.borrow().minibars.borrow().len(), 1);
}

#[test]
fn list_pinned_minibar_labels_returns_tauri_compatible_window_labels() {
    let root = test_app_root();
    seed_test_zone(&root, 1, "Docs");
    seed_test_zone(&root, 2, "Projects");

    assert!(pin_zone_minibar_state(&root, ZoneId(1)).is_some());
    assert!(pin_zone_minibar_state(&root, ZoneId(2)).is_some());

    let labels = list_pinned_minibar_labels(&root);
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0].as_str(), "minibar-1");
    assert_eq!(labels[1].as_str(), "minibar-2");
    assert!(!labels.iter().any(|label| label.as_str().contains("Docs")));
}

#[test]
fn list_pinned_minibars_command_reports_visible_status() {
    let root = test_app_root();
    seed_test_zone(&root, 1, "Docs");
    let _ = restore_minibar_pins_from_wire_value(&root, "1", |root, zone_id| {
        pin_zone_minibar_state(root, zone_id).is_some()
    });

    root.dispatcher.push(Command::ListPinnedMinibars);
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    let status = app.item_operation_status.borrow();
    assert_eq!(
        status.as_ref().map(SmolStr::as_str),
        Some("Pinned minibars: minibar-1")
    );
}

#[test]
fn list_pinned_minibars_empty_reports_visible_status() {
    let root = test_app_root();

    root.dispatcher.push(Command::ListPinnedMinibars);
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    let status = app.item_operation_status.borrow();
    assert_eq!(
        status.as_ref().map(SmolStr::as_str),
        Some("No pinned minibars")
    );
}

#[test]
fn minibar_pins_write_to_vault_round_trips_and_removes_empty_key() {
    let root = test_app_root();
    seed_test_zone(&root, 1, "Docs");
    seed_test_zone(&root, 2, "Projects");
    let _ = restore_minibar_pins_from_wire_value(&root, "1,2", |root, zone_id| {
        pin_zone_minibar_state(root, zone_id).is_some()
    });
    let vault_path = scratch_zones_path("minibar-vault").with_file_name("settings.vault");

    {
        let mut vault = Vault::open(&vault_path).expect("open vault");
        write_minibar_pins_to_vault(&root, &mut vault);
        vault.flush().expect("flush vault");
    }
    let reopened = Vault::open(&vault_path).expect("reopen vault");
    match reopened.get_setting(SETTING_MINIBAR_PINNED_ZONES) {
        Some(bento_nano_backend::config_vault::SettingValue::Str(value)) => {
            assert_eq!(value.as_str(), "1,2");
        }
        other => panic!("expected persisted minibar pins, got {other:?}"),
    }

    assert!(unpin_zone_minibar(&root, ZoneId(1)));
    assert!(unpin_zone_minibar(&root, ZoneId(2)));
    {
        let mut vault = Vault::open(&vault_path).expect("reopen mutable vault");
        write_minibar_pins_to_vault(&root, &mut vault);
        vault.flush().expect("flush removed pins");
    }
    let cleared = Vault::open(&vault_path).expect("reopen cleared vault");
    assert_eq!(cleared.get_setting(SETTING_MINIBAR_PINNED_ZONES), None);
    let _ = std::fs::remove_dir_all(vault_path.parent().expect("vault parent"));
}

fn file_info(path: &std::path::Path) -> FileInfo {
    FileInfo {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: path.to_string_lossy().to_string(),
        size: std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0),
        file_type: path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("unknown")
            .to_string(),
        modified_at: bento_nano_backend::time::now_rfc3339(),
        created_at: bento_nano_backend::time::now_rfc3339(),
        is_directory: path.is_dir(),
        extension: path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(ToOwned::to_owned),
    }
}

fn smart_group_sample_files() -> Vec<FileInfo> {
    (0..4)
        .map(|index| file_info(std::path::Path::new(&format!("C:/Desktop/doc{index}.pdf"))))
        .collect()
}

#[test]
fn search_query_indexes_live_zone_items_settings_and_actions() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(77), "Contracts", 0, 0, 240, 160);
        let _item_id = zone.add_item(
            Cow::Owned("C:/Desktop/contract.pdf".to_owned()),
            Cow::Borrowed("hash"),
        );
        app.zones.add(zone);
    }

    let count = super::run_search_query(&root, "contract");

    assert!(count >= 2, "zone and item should both be searchable");
    let app = root.app.borrow();
    let results = app.search_bar.borrow();
    assert!(
        results
            .results
            .iter()
            .any(|hit| hit.id.as_str() == "zone:77")
    );
    assert!(
        results
            .results
            .iter()
            .any(|hit| hit.id.as_str() == "item:77:1")
    );

    drop(results);
    drop(app);
    let action_count = super::run_search_query(&root, "bulk");
    assert!(action_count >= 1);
    let app = root.app.borrow();
    assert!(
        app.search_bar
            .borrow()
            .results
            .iter()
            .any(|hit| hit.id.as_str() == "action:open_bulk_manager")
    );
}

#[test]
fn search_zone_breadcrumb_is_clean_and_localized() {
    let zh = super::search_zone_breadcrumb(7, 2, true, true);
    assert_eq!(zh, "区域 7 · 2 个项目 · 显示");
    assert!(!zh.contains('路'));

    let en_one = super::search_zone_breadcrumb(7, 1, false, false);
    assert_eq!(en_one, "Zone 7 · 1 item · hidden");
    let en_many = super::search_zone_breadcrumb(7, 2, true, false);
    assert_eq!(en_many, "Zone 7 · 2 items · visible");
}

#[test]
fn search_query_status_is_complete_in_chinese_and_english() {
    assert_eq!(
        super::search_query_status("文档", 0, true),
        "未找到“文档”的匹配结果"
    );
    assert_eq!(
        super::search_query_status("docs", 0, false),
        "No results for \"docs\""
    );
    assert_eq!(
        super::search_query_status("文档", 2, true),
        "找到 2 个实时结果"
    );
    assert_eq!(
        super::search_query_status("docs", 1, false),
        "1 live result"
    );
    assert_eq!(
        super::search_query_status("docs", 2, false),
        "2 live results"
    );
}

#[test]
fn search_activation_selects_live_zone() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(77), "Contracts", 0, 0, 240, 160));
    }
    let _count = super::run_search_query(&root, "contracts");

    assert!(super::activate_search_hit(
        &root,
        "zone:77",
        std::ptr::null_mut()
    ));

    let app = root.app.borrow();
    assert_eq!(app.selected_zone.get(), Some(ZoneId(77)));
    assert_eq!(app.hovered_zone.get(), Some(ZoneId(77)));
    let overlay = app.highlight_overlay.borrow();
    assert_eq!(overlay.targets().len(), 1);
    assert_eq!(overlay.auto_clear_remaining_ms(), Some(3_000));
}

#[test]
fn search_query_highlights_selected_live_item() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(77), "Inbox", 10, 20, 240, 160);
        let _item_id = zone.add_item(
            Cow::Owned("C:/Desktop/contract.pdf".to_owned()),
            Cow::Borrowed("hash"),
        );
        app.zones.add(zone);
    }

    let count = super::run_search_query(&root, "contract");

    assert!(count >= 1);
    let app = root.app.borrow();
    let overlay = app.highlight_overlay.borrow();
    assert_eq!(overlay.targets().len() + overlay.pulses().len(), 1);
    if let Some(target) = overlay.targets().first() {
        assert!(target.x >= 10.0);
    } else {
        assert!(
            overlay.pulses()[0]
                .name
                .as_str()
                .to_ascii_lowercase()
                .contains("contract")
        );
    }
    assert!(overlay.auto_clear_remaining_ms().is_none());
}

#[test]
fn search_index_accepts_scanned_desktop_files() {
    let mut index = super::SearchIndex::new();
    let files = vec![FileInfo {
        name: "offgrid-contract.pdf".to_owned(),
        path: r"C:\Users\Alice\Desktop\offgrid-contract.pdf".to_owned(),
        size: 12,
        file_type: "pdf".to_owned(),
        modified_at: bento_nano_backend::time::now_rfc3339(),
        created_at: bento_nano_backend::time::now_rfc3339(),
        is_directory: false,
        extension: Some("pdf".to_owned()),
    }];

    super::add_desktop_file_search_items(&mut index, &files);

    let hits = index.query("offgrid", 8);
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].id.as_str(),
        r"desktop:C:\Users\Alice\Desktop\offgrid-contract.pdf"
    );
    assert_eq!(
        hits[0].path.as_str(),
        r"C:\Users\Alice\Desktop\offgrid-contract.pdf"
    );
}

#[test]
fn offgrid_desktop_highlight_uses_icon_position_backup() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("offgrid-highlight");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
    }
    let layout = bento_nano_backend::icon_positions::SavedIconLayout {
        icons: vec![bento_nano_backend::icon_positions::IconPosition {
            name: "offgrid-contract.pdf".to_owned(),
            x: 320,
            y: 144,
        }],
        saved_at: bento_nano_backend::time::now_rfc3339(),
        resolution: bento_nano_backend::icon_positions::Resolution {
            width: 1920,
            height: 1080,
        },
        dpi: 1.0,
    };
    bento_nano_backend::icon_positions::persist_to_file(&layout, state_dir)
        .expect("persist icon layout");
    let paths = vec![r"C:\Users\Alice\Desktop\offgrid-contract.pdf".to_owned()];

    let highlighted = super::set_highlight_for_paths(&root, &paths);

    assert_eq!(highlighted, 1);
    let app = root.app.borrow();
    let overlay = app.highlight_overlay.borrow();
    assert!(overlay.targets().is_empty());
    assert_eq!(overlay.pulses().len(), 1);
    assert_eq!(overlay.pulses()[0].name.as_str(), "offgrid-contract.pdf");
    assert_eq!(overlay.pulses()[0].x, 320.0);
    assert_eq!(overlay.pulses()[0].y, 144.0);
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn search_icon_for_kind_returns_selected_stack_icon_slugs() {
    for kind in [
        SearchItemKind::File,
        SearchItemKind::Folder,
        SearchItemKind::Zone,
        SearchItemKind::Setting,
        SearchItemKind::Action,
    ] {
        let slug = search_icon_for_kind(&kind);
        assert!(
            IconKind::from_str_opt(slug.as_str()).is_some(),
            "search icon must render through built-in glyph path: {slug}"
        );
    }
}
