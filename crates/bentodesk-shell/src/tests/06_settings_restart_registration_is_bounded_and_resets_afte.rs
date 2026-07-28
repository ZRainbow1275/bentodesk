#[test]
fn settings_restart_registration_is_bounded_and_resets_after_window() {
    assert_eq!(
        super::restart_registration_command(false, 3, 60, 120, &[]),
        None
    );
    assert_eq!(
        super::restart_registration_command(true, 0, 60, 120, &[]),
        None
    );
    let args = vec![
        "bentodesk-shell.exe".to_owned(),
        format!("{}1", super::RESTART_ATTEMPT_ARG),
        format!("{}100", super::RESTART_WINDOW_START_ARG),
    ];
    assert_eq!(
        super::restart_registration_command(true, 3, 60, 150, &args).as_deref(),
        Some("--bentodesk-restart-attempt=2 --bentodesk-restart-window-start=100")
    );
    let exhausted = vec![
        format!("{}3", super::RESTART_ATTEMPT_ARG),
        format!("{}100", super::RESTART_WINDOW_START_ARG),
    ];
    assert_eq!(
        super::restart_registration_command(true, 3, 60, 150, &exhausted),
        None
    );
    assert_eq!(
        super::restart_registration_command(true, 3, 60, 161, &args).as_deref(),
        Some("--bentodesk-restart-attempt=1 --bentodesk-restart-window-start=161")
    );
}

#[test]
fn settings_transaction_persists_every_save_gated_field() {
    use bentodesk_backend::config_vault::SettingValue;

    let scratch = std::env::temp_dir().join(format!(
        "bentodesk-settings-vault-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&scratch).expect("create vault scratch");
    let vault_path = scratch.join("vault.bin");
    let snapshot = SettingsSnapshot {
        ghost_layer_enabled: false,
        launch_at_startup: true,
        show_in_taskbar: true,
        auto_group_enabled: false,
        portable_mode: true,
        expand_delay_ms: 175,
        collapse_delay_ms: 425,
        icon_cache_size: 777,
        startup_high_priority: true,
        crash_restart_enabled: true,
        crash_max_retries: 4,
        crash_window_secs: 88,
        safe_start_after_hibernation: false,
        hibernate_resume_delay_ms: 1_250,
        active_theme_id: SmolStr::new_static("light"),
        zone_display_mode: ZoneDisplayMode::Click,
        desktop_path_draft: SmolStr::new_static(r"C:\Users\Test\Desktop"),
        watch_paths_draft: SmolStr::new_static("D:\\Docs\nD:\\Work"),
    };
    let accent = SmolStr::new_static("#22c55e");
    let mut vault = Vault::open(&vault_path).expect("open settings vault");
    super::persist_settings_snapshot_to_vault(&mut vault, &snapshot, Some(&accent), false);

    for key in super::SETTINGS_TRANSACTION_KEYS {
        assert!(
            vault.get_setting(key).is_some(),
            "transaction omitted persisted key {key}"
        );
    }
    assert_eq!(
        vault.get_setting(super::SETTING_GENERAL_LAUNCH_AT_STARTUP),
        Some(SettingValue::Bool(true))
    );
    assert_eq!(
        vault.get_setting(super::SETTING_PERF_ICON_CACHE_SIZE),
        Some(SettingValue::Int(777))
    );
    assert_eq!(
        vault.get_setting(super::SETTING_STARTUP_CRASH_MAX_RETRIES),
        Some(SettingValue::Int(4))
    );
    assert_eq!(
        vault.get_setting(super::SETTING_PATHS_WATCH_PATHS),
        Some(SettingValue::Str(SmolStr::new_static("D:\\Docs\nD:\\Work")))
    );
    assert_eq!(
        vault.get_setting(super::SETTING_ACTIVE_THEME),
        Some(SettingValue::Str(SmolStr::new_static("light")))
    );
    assert_eq!(
        vault.get_setting(super::SETTING_ZONE_DISPLAY_MODE),
        Some(SettingValue::Str(SmolStr::new_static("click")))
    );
    vault.flush().expect("flush settings vault");
    drop(vault);
    let reopened = Vault::open(&vault_path).expect("reopen settings vault");
    assert_eq!(
        reopened.get_setting(super::SETTING_APPEARANCE_ACCENT_COLOR),
        Some(SettingValue::Str(accent))
    );

    let _ = std::fs::remove_dir_all(scratch);
}

#[test]
#[ignore = "requires interactive desktop — creates a real D3D/DComp window via ensure_aux_window; passes on a real desktop (verified 2026-06-01), gated so headless `cargo test` stays green"]
fn about_commands_toggle_runtime_state_without_settings_placeholder() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        assert!(!app.about_open.get());
    }

    root.dispatcher.push(Command::OpenAbout);
    consume_dispatcher(&root, std::ptr::null_mut());
    {
        let app = root.app.borrow();
        assert!(app.about_open.get());
        assert!(!app.settings_open.get());
    }

    root.dispatcher.push(Command::CloseAbout);
    consume_dispatcher(&root, std::ptr::null_mut());
    assert!(!root.app.borrow().about_open.get());
}

fn tooltip_snapshot(
    id: &'static str,
    name: &'static str,
) -> bentodesk_backend::layout::DesktopSnapshot {
    bentodesk_backend::layout::DesktopSnapshot {
        id: SmolStr::new_static(id),
        name: name.to_owned(),
        resolution: bentodesk_backend::layout::Resolution {
            width: 1920,
            height: 1080,
        },
        dpi: 1.0,
        zones: Vec::new(),
        captured_at: SmolStr::new_static("2026-05-12T00:00:00Z"),
    }
}

// V-6 Round-2 (2026-05-21) — `tooltip_hover_toolbar_icon_producer_queues_show_and_hide`
// retired. `mount_main_tree` no longer attaches IconButtons to the
// Main HWND tree, so a hover over the legacy toolbar's SETTINGS
// glyph is no longer a producible event from the Main HWND surface.
// The keybinding action label table
// (`toolbar_action_label("action:open_settings")`) and the
// `Command::ShowTooltip`/`HideTooltip` plumbing remain wired up so
// other call sites (the keybindings_section row hovers, plugin row
// hovers, etc.) keep their tooltip behavior. Hover-over-empty-space
// and hover-over-zone-surface are still covered by
// `tooltip_hover_zone_pill_producer_queues_show_and_hide` (zones) and
// by the trailing `hide` assertion in
// `tooltip_hover_pin_toolbar_text_tracks_pin_state` (also retired).
#[test]
fn _retired_tooltip_hover_toolbar_icon_producer_queues_show_and_hide_v6_r2() {}

// V-6 Round-2 (2026-05-21) — `tooltip_hover_pin_toolbar_text_tracks_pin_state`
// retired. Same reason as the SETTINGS variant above: no IconButton
// is mounted on the Main HWND tree anymore, so there's no toolbar
// PIN glyph rect to hover over. The Pin/Unpin label string-resolution
// function (`toolbar_action_label`) and the `is_pinned` Cell state
// toggle still live in this file and remain reachable through the
// keybindings dispatch table + `Command::TogglePin` consumer in the
// wndproc.
#[test]
fn _retired_tooltip_hover_pin_toolbar_text_tracks_pin_state_v6_r2() {}

// Round-2 M1 — K1 Settings row-tooltip tests retired. The K1 row
// geometry (`settings_update_check_now_rect`, `settings_update_auto_download_rect`)
// is orphan-alive per Ruling B but the centre points now fall on M1's
// top-toggle band, so the tooltip text is M1's "Toggle — wording, not
// the K1 "Check for updates" / "Disable automatic update downloads"
// strings. M4 will fully delete the K1 helpers and these stubs.
#[test]
fn _retired_tooltip_hover_settings_panel_button_in_round_2_m1() {}

#[test]
fn _retired_tooltip_hover_settings_toggle_auto_download_in_round_2_m1() {}

#[test]
fn tooltip_hover_search_row_producer_queues_show_and_hide() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 640.0,
            height: 520.0,
        };
        let mut rows =
            smallvec::SmallVec::<[bentodesk_app::business::search_bar::SearchHit; 8]>::new();
        rows.push(bentodesk_app::business::search_bar::SearchHit {
            id: SmolStr::new_static("zone:77"),
            kind: bentodesk_backend::search::SearchItemKind::Zone,
            name: SmolStr::new_static("Contracts"),
            breadcrumb: SmolStr::new_static("Desktop / Work"),
            icon: SmolStr::new_static("grid"),
            score: 90,
            matched_token: SmolStr::new_static("contracts"),
        });
        app.search_bar.borrow_mut().set_results(rows);
    }
    let (row_x, row_y) = {
        let app = root.app.borrow();
        rect_center(bentodesk_app::business::search_bar::search_row_rect(
            app.viewport,
            0,
        ))
    };

    let show = {
        let app = root.app.borrow();
        tooltip_command_for_search_hover(&app, bentodesk_app::WindowHandle::NULL, row_x, row_y)
    };
    match show {
        Some(Command::ShowTooltip { anchor, text }) => {
            assert_eq!(anchor, bentodesk_app::WindowHandle::NULL);
            assert_eq!(text.as_str(), "Contracts - Desktop / Work");
        }
        other => panic!("expected search row tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        assert!(app.show_tooltip_text(SmolStr::new_static("Contracts - Desktop / Work")));
    }
    let hide = {
        let app = root.app.borrow();
        tooltip_command_for_search_hover(&app, bentodesk_app::WindowHandle::NULL, 0.0, 0.0)
    };
    assert!(matches!(hide, Some(Command::HideTooltip)));
}

#[test]
fn tooltip_hover_search_close_producer_queues_close_text() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 640.0,
            height: 520.0,
        };
    }
    let (close_x, close_y) = {
        let app = root.app.borrow();
        rect_center(bentodesk_app::business::search_bar::search_close_rect(
            app.viewport,
        ))
    };

    let show = {
        let app = root.app.borrow();
        tooltip_command_for_search_hover(&app, bentodesk_app::WindowHandle::NULL, close_x, close_y)
    };
    match show {
        Some(Command::ShowTooltip { text, .. }) => assert_eq!(text.as_str(), "Close search"),
        other => panic!("expected search close tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_bulk_manager_button_producer_queues_show_and_hide() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        app.zones.add(Zone::new(ZoneId(11), "Docs", 0, 0, 240, 160));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
    }
    let update_spec = bentodesk_app::business::bulk_manager_panel::BULK_MANAGER_ACTION_BUTTONS
        .iter()
        .copied()
        .find(|spec| {
            spec.hit == bentodesk_app::business::bulk_manager_panel::BulkManagerPointerHit::Update
        })
        .expect("update button spec");
    let (update_x, update_y) = {
        let app = root.app.borrow();
        rect_center(
            bentodesk_app::business::bulk_manager_panel::bulk_manager_button_rect(
                app.viewport,
                update_spec,
            ),
        )
    };

    let show = {
        let app = root.app.borrow();
        tooltip_command_for_bulk_manager_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            update_x,
            update_y,
        )
    };
    match show {
        Some(Command::ShowTooltip { anchor, text }) => {
            assert_eq!(anchor, bentodesk_app::WindowHandle::NULL);
            assert_eq!(text.as_str(), "Apply metadata updates");
        }
        other => panic!("expected BulkManager update tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        assert!(app.show_tooltip_text(SmolStr::new_static("Apply metadata updates")));
    }
    let hide = {
        let app = root.app.borrow();
        tooltip_command_for_bulk_manager_hover(&app, bentodesk_app::WindowHandle::NULL, 0.0, 0.0)
    };
    assert!(matches!(hide, Some(Command::HideTooltip)));
}

#[test]
fn tooltip_hover_bulk_manager_sort_header_producer_queues_sort_text() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        app.zones.add(Zone::new(ZoneId(11), "Docs", 0, 0, 240, 160));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
    }
    let (sort_x, sort_y) = {
        let app = root.app.borrow();
        rect_center(
            bentodesk_app::business::bulk_manager_panel::bulk_manager_sort_header_rect(
                app.viewport,
                SortKey::Items,
            ),
        )
    };

    let show = {
        let app = root.app.borrow();
        tooltip_command_for_bulk_manager_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            sort_x,
            sort_y,
        )
    };
    match show {
        Some(Command::ShowTooltip { anchor, text }) => {
            assert_eq!(anchor, bentodesk_app::WindowHandle::NULL);
            assert_eq!(text.as_str(), "Sort bulk rows by Items");
        }
        other => panic!("expected BulkManager sort tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_bulk_manager_row_text_tracks_selection_state() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        let mut zone = Zone::new(ZoneId(12), "Docs", 0, 0, 240, 160);
        let _ = zone.add_item(
            Cow::Owned("C:/Desktop/report.pdf".to_owned()),
            Cow::Borrowed("hash"),
        );
        app.zones.add(zone);
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
    }
    let (row_x, row_y) = {
        let app = root.app.borrow();
        rect_center(
            bentodesk_app::business::bulk_manager_panel::bulk_manager_row_rect(app.viewport, 0),
        )
    };

    let unselected = {
        let app = root.app.borrow();
        tooltip_command_for_bulk_manager_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            row_x,
            row_y,
        )
    };
    match unselected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Select Docs (1 item)");
        }
        other => panic!("expected unselected BulkManager row tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        app.bulk_manager
            .borrow_mut()
            .toggle_visible_row_selection(0);
    }
    let selected = {
        let app = root.app.borrow();
        tooltip_command_for_bulk_manager_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            row_x,
            row_y,
        )
    };
    match selected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Deselect Docs (1 item)");
        }
        other => panic!("expected selected BulkManager row tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_rules_wizard_button_text_tracks_step() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
    }
    let next = rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::NextSave);
    let (next_x, next_y) = rect_center(next);

    let conditions = {
        let app = root.app.borrow();
        tooltip_command_for_rules_wizard_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            next_x,
            next_y,
        )
    };
    match conditions {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Continue to rule action");
        }
        other => panic!("expected RulesWizard conditions tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        app.rules_wizard
            .borrow_mut()
            .load_rule(sample_rule("rule-tooltip"));
        app.rules_wizard.borrow_mut().click_next();
    }
    let action = {
        let app = root.app.borrow();
        tooltip_command_for_rules_wizard_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            next_x,
            next_y,
        )
    };
    match action {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Preview matching files");
        }
        other => panic!("expected RulesWizard action tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_rules_wizard_row_text_tracks_cursor_and_hide() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        let mut first = sample_rule("rule-one");
        first.name = "First rule".to_string();
        let mut second = sample_rule("rule-two");
        second.name = "Second rule".to_string();
        *app.rules_wizard_rules.borrow_mut() = vec![first, second];
        app.rules_wizard_rule_cursor.set(0);
    }
    let (row_x, row_y) = {
        let app = root.app.borrow();
        rect_center(rules_wizard::rules_wizard_rule_row_rect(app.viewport, 1))
    };

    let unselected = {
        let app = root.app.borrow();
        tooltip_command_for_rules_wizard_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            row_x,
            row_y,
        )
    };
    match unselected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Select rule Second rule");
        }
        other => panic!("expected unselected RulesWizard row tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        app.rules_wizard_rule_cursor.set(1);
        assert!(app.show_tooltip_text(SmolStr::new_static("Select rule Second rule")));
    }
    let selected = {
        let app = root.app.borrow();
        tooltip_command_for_rules_wizard_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            row_x,
            row_y,
        )
    };
    match selected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Selected rule Second rule");
        }
        other => panic!("expected selected RulesWizard row tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        assert!(app.show_tooltip_text(SmolStr::new_static("Selected rule Second rule")));
    }
    let hide = {
        let app = root.app.borrow();
        tooltip_command_for_rules_wizard_hover(&app, bentodesk_app::WindowHandle::NULL, 0.0, 0.0)
    };
    assert!(matches!(hide, Some(Command::HideTooltip)));
}

#[test]
fn tooltip_hover_rules_wizard_row_uses_scrolled_rule_window() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        let mut rules = Vec::new();
        for index in 0..8 {
            let mut rule = sample_rule(&format!("rule-{index:02}"));
            rule.name = format!("Rule {index:02}");
            rules.push(rule);
        }
        *app.rules_wizard_rules.borrow_mut() = rules;
        app.rules_wizard_rule_cursor.set(6);
    }
    let (row_x, row_y) = {
        let app = root.app.borrow();
        rect_center(rules_wizard::rules_wizard_rule_row_rect(app.viewport, 0))
    };

    let scrolled = {
        let app = root.app.borrow();
        tooltip_command_for_rules_wizard_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            row_x,
            row_y,
        )
    };
    match scrolled {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Select rule Rule 01");
        }
        other => panic!("expected scrolled RulesWizard row tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_timeline_button_producer_queues_show_and_hide() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
    }
    let save = bentodesk_app::business::timeline::panel::TIMELINE_ACTION_BUTTONS
        .iter()
        .copied()
        .find(|spec| {
            spec.hit == bentodesk_app::business::timeline::panel::TimelinePointerHit::Save
        })
        .expect("timeline save button");
    let (save_x, save_y) = {
        let app = root.app.borrow();
        rect_center(
            bentodesk_app::business::timeline::panel::timeline_button_rect(app.viewport, save),
        )
    };

    let show = {
        let app = root.app.borrow();
        tooltip_command_for_timeline_hover(&app, bentodesk_app::WindowHandle::NULL, save_x, save_y)
    };
    match show {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Save manual checkpoint");
        }
        other => panic!("expected Timeline save tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        assert!(app.show_tooltip_text(SmolStr::new_static("Save manual checkpoint")));
    }
    let hide = {
        let app = root.app.borrow();
        tooltip_command_for_timeline_hover(&app, bentodesk_app::WindowHandle::NULL, 0.0, 0.0)
    };
    assert!(matches!(hide, Some(Command::HideTooltip)));
}
