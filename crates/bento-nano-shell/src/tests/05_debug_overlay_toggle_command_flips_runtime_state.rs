#[test]
fn debug_overlay_toggle_command_flips_runtime_state() {
    let root = test_app_root();
    assert!(!root.app.borrow().debug_overlay.borrow().visible);

    root.dispatcher.push(Command::ToggleDebugOverlay);
    consume_dispatcher(&root, std::ptr::null_mut());
    assert!(root.app.borrow().debug_overlay.borrow().visible);

    root.dispatcher.push(Command::ToggleDebugOverlay);
    consume_dispatcher(&root, std::ptr::null_mut());
    assert!(!root.app.borrow().debug_overlay.borrow().visible);
}

/// M7 — WM_CHAR routes a typed char into the focused 妗岄潰璺緞 draft and
/// marks the panel dirty. Pure CPU (null HWND, no window/D3D). Not focused →    /// rejected.
#[test]
fn m7_handle_settings_text_char_appends_to_desktop_path() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        // Start from a clean draft so the assertion is exact.
        *app.desktop_path_draft.borrow_mut() = smol_str::SmolStr::default();
        app.settings_dirty.set(false);
    }
    // No field focused → the char is NOT consumed by the text handler.
    assert!(!handle_settings_text_char(&root, u32::from('C')));
    assert_eq!(root.app.borrow().desktop_path_draft.borrow().as_str(), "");

    // Focus DesktopPath → 'C' then ':' append; panel goes dirty.
    root.app
        .borrow()
        .settings_focused_field
        .set(bento_nano_app::SettingsTextField::DesktopPath);
    assert!(handle_settings_text_char(&root, u32::from('C')));
    assert!(handle_settings_text_char(&root, u32::from(':')));
    let app = root.app.borrow();
    assert_eq!(app.desktop_path_draft.borrow().as_str(), "C:");
    assert!(app.settings_dirty.get(), "typing must mark settings dirty");
}

/// M7 — WM_KEYDOWN: Backspace pops the last char from the focused draft;
/// Esc blurs the field and falls through so Settings closes on that same
/// keydown. Pure CPU (null HWND).
#[test]
fn m7_handle_settings_text_keydown_backspace_and_blur() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        *app.watch_paths_draft.borrow_mut() = smol_str::SmolStr::new("ab");
        app.settings_focused_field
            .set(bento_nano_app::SettingsTextField::WatchValues);
    }
    // Backspace pops one scalar → "a".
    assert_eq!(
        handle_settings_text_keydown(&root, VK_BACKSPACE, std::ptr::null_mut()),
        Some(0)
    );
    assert_eq!(root.app.borrow().watch_paths_draft.borrow().as_str(), "a");

    // Enter on the WatchValues textarea inserts a newline (not a blur).
    assert_eq!(
        handle_settings_text_keydown(&root, VK_ENTER, std::ptr::null_mut()),
        Some(0)
    );
    assert_eq!(root.app.borrow().watch_paths_draft.borrow().as_str(), "a\n");

    // Esc blurs the field but is not consumed: `handle_keydown` must let
    // the same key reach the Settings auxiliary-escape close branch.
    assert_eq!(
        handle_settings_text_keydown(&root, VK_ESCAPE_KEY, std::ptr::null_mut()),
        None
    );
    assert_eq!(
        root.app.borrow().settings_focused_field.get(),
        bento_nano_app::SettingsTextField::None
    );

    // With no field focused, the text keydown returns None so the
    // passphrase + auxiliary-escape paths still run.
    assert_eq!(
        handle_settings_text_keydown(&root, VK_BACKSPACE, std::ptr::null_mut()),
        None
    );
}

/// M7 — Enter on the single-line DesktopPath field blurs it (does NOT insert
/// a newline, unlike the WatchValues textarea).
#[test]
fn m7_handle_settings_text_keydown_enter_blurs_single_line_path() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        *app.desktop_path_draft.borrow_mut() = smol_str::SmolStr::new("D:");
        app.settings_focused_field
            .set(bento_nano_app::SettingsTextField::DesktopPath);
    }
    assert_eq!(
        handle_settings_text_keydown(&root, VK_ENTER, std::ptr::null_mut()),
        Some(0)
    );
    let app = root.app.borrow();
    // Draft unchanged (no newline), field blurred.
    assert_eq!(app.desktop_path_draft.borrow().as_str(), "D:");
    assert_eq!(
        app.settings_focused_field.get(),
        bento_nano_app::SettingsTextField::None
    );
}

#[test]
fn v21_n15_handle_settings_text_char_appends_to_accent_color() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        *app.settings_draft_accent_color.borrow_mut() = None;
        app.settings_focused_field
            .set(bento_nano_app::SettingsTextField::AccentColor);
        app.settings_dirty.set(false);
    }

    assert!(handle_settings_text_char(&root, u32::from('A')));
    assert!(handle_settings_text_char(&root, u32::from('b')));
    {
        let app = root.app.borrow();
        assert_eq!(
            app.settings_draft_accent_color.borrow().as_deref(),
            Some("#ab")
        );
        assert!(app.settings_dirty.get());
        app.settings_dirty.set(false);
    }

    assert!(handle_settings_text_char(&root, u32::from('g')));
    let app = root.app.borrow();
    assert_eq!(
        app.settings_draft_accent_color.borrow().as_deref(),
        Some("#ab")
    );
    assert!(
        !app.settings_dirty.get(),
        "rejected chars must not mark dirty"
    );
}

#[test]
fn v21_n15_handle_settings_text_keydown_enter_blurs_accent_color() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        *app.settings_draft_accent_color.borrow_mut() = Some(smol_str::SmolStr::new("#abcdef"));
        app.settings_focused_field
            .set(bento_nano_app::SettingsTextField::AccentColor);
    }
    assert_eq!(
        handle_settings_text_keydown(&root, VK_ENTER, std::ptr::null_mut()),
        Some(0)
    );
    let app = root.app.borrow();
    assert_eq!(
        app.settings_draft_accent_color.borrow().as_deref(),
        Some("#abcdef")
    );
    assert_eq!(
        app.settings_focused_field.get(),
        bento_nano_app::SettingsTextField::None
    );
}

#[test]
fn v21_n16_native_colorref_round_trips_accent_hex() {
    assert_eq!(accent_hex_to_colorref("#3b82f6"), Some(0x00F6_823B));
    assert_eq!(accent_hex_to_colorref("#14B8a6"), Some(0x00A6_B814));
    assert_eq!(accent_hex_to_colorref("3b82f6"), None);
    assert_eq!(accent_hex_to_colorref("#3b82fg"), None);
    assert_eq!(colorref_to_accent_hex(0x00A6_B814).as_str(), "#14b8a6");
}

/// W3 (#7 fix wave 2026-06-01) — the settings keydowns must route for BOTH
/// Main and the focusable Settings aux HWND; every other aux kind and the
/// stack tray must NOT. Pins the routing predicate so a future refactor
/// can't silently drop the Settings branch (the latent bug this fix closed).
#[test]
fn w3_settings_keydown_routes_for_main_and_settings_only() {
    assert!(window_kind_routes_settings_keydown(WindowKind::Main));
    assert!(window_kind_routes_settings_keydown(WindowKind::Settings));
    // A representative sample of NON-settings kinds must NOT route here.
    assert!(!window_kind_routes_settings_keydown(WindowKind::ZoneEditor));
    assert!(!window_kind_routes_settings_keydown(WindowKind::IconPicker));
    assert!(!window_kind_routes_settings_keydown(WindowKind::Search));
    assert!(!window_kind_routes_settings_keydown(WindowKind::About));
}

#[test]
fn w13_settings_dismiss_resets_scroll_for_next_open() {
    let root = test_app_root();
    let app = root.app.borrow();
    app.scroll_offset_y.set(720.0);
    app.settings_plugin_uninstall_confirm.set(Some(0));

    reset_settings_transient_state(&app);

    assert_eq!(app.scroll_offset_y.get(), 0.0);
    assert_eq!(app.settings_plugin_uninstall_confirm.get(), None);
}

#[test]
fn w3_settings_pointer_uses_aux_window_when_registered() {
    assert!(window_kind_routes_settings_pointer(WindowKind::Main, false));
    assert!(window_kind_routes_settings_pointer(
        WindowKind::Settings,
        true
    ));
    assert!(!window_kind_routes_settings_pointer(WindowKind::Main, true));
    assert!(!window_kind_routes_settings_pointer(
        WindowKind::Search,
        true
    ));
}

#[test]
fn w3_settings_mousewheel_delta_uses_native_wheel_direction() {
    fn wheel_wparam(delta: i16) -> WPARAM {
        ((delta as u16 as usize) << 16) as WPARAM
    }

    assert_eq!(
        settings_wheel_scroll_delta_from_wparam(wheel_wparam(120)),
        Some(-120),
        "wheel-up must move the Settings body toward the top"
    );
    assert_eq!(
        settings_wheel_scroll_delta_from_wparam(wheel_wparam(-120)),
        Some(120),
        "wheel-down must increase the Settings body scroll offset"
    );
    assert_eq!(
        settings_wheel_scroll_delta_from_wparam(wheel_wparam(0)),
        None
    );
}

#[test]
fn expanded_zone_wheel_target_is_content_only_and_query_aware() {
    let mut app = AppState::new();
    let mut zone = Zone::new(ZoneId(4), "Benchmark Zone 4", 64, 332, 320, 220);
    zone.set_grid_columns(5);
    for index in 1..=10 {
        zone.add_item(
            format!("C:/Desktop/item-{index:02}.txt"),
            format!("hash-{index:02}"),
        )
        .expect("benchmark item");
    }
    app.zones.add(zone);
    app.set_zone_display_mode(ZoneDisplayMode::Always);

    let zone = app.zones.get(ZoneId(4)).expect("zone");
    assert!(zone_item_max_scroll(&app, zone) > 0.0);
    assert_eq!(zone_scroll_target_for_point(&app, 100.0, 350.0), None);
    let target = zone_scroll_target_for_point(&app, 100.0, 400.0).expect("content target");
    assert_eq!(target.0, ZoneId(4));
    assert!(target.1 > 0.0);

    app.zone_search_target.set(Some(ZoneId(4)));
    app.search_bar.borrow_mut().query = "item-01".into();
    assert_eq!(zone_item_max_scroll(&app, zone), 0.0);
    assert_eq!(zone_scroll_target_for_point(&app, 100.0, 410.0), None);
    assert_eq!(
        zone_scroll_target_for_point(&app, 100.0, 450.0),
        Some((ZoneId(4), 0.0))
    );
}

#[test]
fn w3_settings_input_viewport_uses_current_window_slot_size() {
    let logical = logical_viewport_from_device_size(800, 600, 96);
    assert_eq!(logical.width, 800.0);
    assert_eq!(logical.height, 600.0);

    let hidpi = logical_viewport_from_device_size(1_200, 900, 144);
    assert!((hidpi.width - 800.0).abs() < f32::EPSILON);
    assert!((hidpi.height - 600.0).abs() < f32::EPSILON);
}

#[test]
fn auxiliary_input_viewport_projection_restores_main_and_nests() {
    let root = test_app_root();
    let main = Size {
        width: 1_706.0,
        height: 900.0,
    };
    let editor = Size {
        width: 480.0,
        height: 460.0,
    };
    let picker = Size {
        width: 320.0,
        height: 240.0,
    };
    root.app.borrow_mut().viewport = main;

    with_app_viewport(&root, editor, || {
        assert_eq!(root.app.borrow().viewport, editor);
        with_app_viewport(&root, picker, || {
            assert_eq!(root.app.borrow().viewport, picker);
        });
        assert_eq!(root.app.borrow().viewport, editor);
    });

    assert_eq!(root.app.borrow().viewport, main);
}

#[test]
fn settings_aux_host_is_panel_sized_and_centered_at_150_percent_dpi() {
    let work = bento_nano_platform::RectI32 {
        left: 0,
        top: 0,
        right: 2560,
        bottom: 1368,
    };
    assert_eq!(settings_aux_host_rect(work, 144), (920, 137, 720, 1094));
}

#[test]
fn settings_aux_host_clamps_to_eighty_percent_of_small_workarea() {
    let work = bento_nano_platform::RectI32 {
        left: 10,
        top: 20,
        right: 1010,
        bottom: 720,
    };
    assert_eq!(settings_aux_host_rect(work, 96), (270, 90, 480, 560));
}

#[test]
fn settings_aux_host_stays_within_offset_workarea_at_200_percent_dpi() {
    let work = bento_nano_platform::RectI32 {
        left: -1920,
        top: 40,
        right: 0,
        bottom: 1120,
    };
    let (x, y, width, height) = settings_aux_host_rect(work, 192);
    assert_eq!((x, y, width, height), (-1440, 148, 960, 864));
    assert!(x >= work.left && y >= work.top);
    assert!(x + width <= work.right && y + height <= work.bottom);
}

#[test]
fn zone_editor_host_is_dpi_scaled_and_centered_in_workarea() {
    let work = bento_nano_platform::RectI32 {
        left: 0,
        top: 0,
        right: 2560,
        bottom: 1368,
    };
    assert_eq!(
        centered_fixed_aux_host_rect(work, 144, 480.0, 460.0),
        (920, 339, 720, 690)
    );
}

/// W3 — once routing reaches the Settings HWND, typing must actually land in
/// the focused passphrase draft. Drive `handle_settings_passphrase_char`
/// (the seam the aux WM_CHAR Settings branch now calls) with an active
/// passphrase capture and assert the draft grows. Pure CPU (no window).
#[test]
fn w3_passphrase_char_lands_when_settings_focused() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        focus_passphrase_field(&app);
        app.passphrase_draft.borrow_mut().clear();
    }
    assert!(handle_settings_passphrase_char(&root, u32::from('p')));
    assert!(handle_settings_passphrase_char(&root, u32::from('w')));
    assert_eq!(root.app.borrow().passphrase_draft.borrow().as_str(), "pw");
}

/// P15 (#7 fix wave 2026-06-01) — clicking the passphrase INPUT is PURE
/// FOCUS: it activates capture + focuses the field but NEVER applies a mode,
/// NEVER clears the draft, and pushes NO command. Typing must still work
/// after the focus.
#[test]
fn p15_focus_passphrase_field_is_pure_focus() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        app.encryption_mode.set(SettingsEncryptionMode::None);
        // Pre-existing typed draft must survive a (re)focus.
        *app.passphrase_draft.borrow_mut() = "secret".to_owned();
        focus_passphrase_field(&app);
    }
    let app = root.app.borrow();
    assert!(app.passphrase_entry_active.get(), "capture activated");
    assert_eq!(
        app.settings_focused_field.get(),
        bento_nano_app::SettingsTextField::Passphrase
    );
    // Pure focus: mode unchanged, draft preserved, no command queued.
    assert_eq!(app.encryption_mode.get(), SettingsEncryptionMode::None);
    assert_eq!(app.passphrase_draft.borrow().as_str(), "secret");
    assert!(
        app.settings_encryption_status.borrow().is_none(),
        "P10 — no prompt/status banner on pure focus"
    );
}

/// P15 — the Passphrase BUTTON applies. Empty draft → localized
/// ENCRYPTION_REQUIRED error + NO command. Non-empty draft → the
/// verify-probe→apply command + the in-flight capture is cleared.
#[test]
fn p15_passphrase_button_empty_sets_required_error_and_returns_none() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        app.passphrase_unlock_required.set(false);
        app.passphrase_draft.borrow_mut().clear();
    }
    let app = root.app.borrow();
    let command = passphrase_button_command(&app);
    assert!(command.is_none(), "empty draft must NOT push a command");
    let status = app.settings_encryption_status.borrow();
    match status.as_ref() {
        Some(SettingsBackupStatus::Error(msg)) => assert_eq!(
            msg.as_str(),
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_REQUIRED)
        ),
        other => panic!("expected localized ENCRYPTION_REQUIRED error, got {other:?}"),
    }
}

#[test]
fn p15_passphrase_button_set_returns_set_command_and_clears_capture() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        app.passphrase_unlock_required.set(false);
        *app.passphrase_draft.borrow_mut() = "  hunter2  ".to_owned();
        app.passphrase_entry_active.set(true);
        app.settings_focused_field
            .set(bento_nano_app::SettingsTextField::Passphrase);
    }
    let app = root.app.borrow();
    match passphrase_button_command(&app) {
        Some(Command::SetEncryptionPassphrase(pw)) => {
            // Trimmed before commit.
            assert_eq!(pw.as_str(), "hunter2");
        }
        other => panic!("expected SetEncryptionPassphrase, got {other:?}"),
    }
    // In-flight capture cleared + field blurred.
    assert!(!app.passphrase_entry_active.get());
    assert!(app.passphrase_draft.borrow().is_empty());
    assert_eq!(
        app.settings_focused_field.get(),
        bento_nano_app::SettingsTextField::None
    );
}

#[test]
fn p15_passphrase_button_unlock_returns_unlock_command() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        // Unlock-required → the button routes to the unlock command.
        app.passphrase_unlock_required.set(true);
        *app.passphrase_draft.borrow_mut() = "openme".to_owned();
    }
    let app = root.app.borrow();
    assert!(matches!(
        passphrase_button_command(&app),
        Some(Command::UnlockEncryptionPassphrase(_))
    ));
}

/// W1 (#7 fix wave 2026-06-01) — `save_settings_general` captures the two §2
/// Paths drafts under the same borrow it batches the toggles with. The
/// process-global vault isn't installed in a headless test, so Save must
/// reject the transaction without lying that it succeeded. Pin that the
/// drafts are read (no panic / no borrow conflict), remain editable, and
/// retain a visible error while dirty stays true.
#[test]
fn w1_save_settings_general_captures_path_drafts_without_panicking() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        app.settings_dirty.set(true);
        *app.desktop_path_draft.borrow_mut() =
            smol_str::SmolStr::new(std::env::temp_dir().to_string_lossy().as_ref());
        *app.watch_paths_draft.borrow_mut() = SmolStr::new_static("");
    }
    let saved = save_settings_general(&root, std::ptr::null_mut());
    let app = root.app.borrow();
    assert!(!saved);
    assert!(
        app.settings_dirty.get(),
        "failed Save must retain unsaved changes"
    );
    assert_eq!(
        app.desktop_path_draft.borrow().as_str(),
        std::env::temp_dir().to_string_lossy().as_ref()
    );
    assert!(app.watch_paths_draft.borrow().is_empty());
    assert!(
        app.settings_save_error.borrow().is_some(),
        "failed Save must remain visible in the Settings footer"
    );
}

#[test]
fn settings_source_validation_accepts_real_dirs_dedupes_and_rejects_files() {
    let scratch = std::env::temp_dir().join(format!(
        "bento-nano-settings-sources-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let desktop = scratch.join("Desktop");
    let watch = scratch.join("Watch");
    std::fs::create_dir_all(&desktop).expect("create desktop source");
    std::fs::create_dir_all(&watch).expect("create watch source");
    let file = scratch.join("not-a-directory.txt");
    std::fs::write(&file, b"file").expect("create plain file");

    let mut snapshot = AppState::new().snapshot_settings();
    snapshot.desktop_path_draft = SmolStr::new(desktop.to_string_lossy().as_ref());
    snapshot.watch_paths_draft = SmolStr::new(format!(
        "{}\n{}\n{}",
        desktop.display(),
        watch.display(),
        watch.display()
    ));
    let sources =
        super::validate_settings_sources_for_locale(&snapshot, true).expect("valid source list");
    let canonical_desktop = std::fs::canonicalize(&desktop).expect("canonical desktop");
    let canonical_watch = std::fs::canonicalize(&watch).expect("canonical watch");
    assert_eq!(
        sources
            .iter()
            .filter(|source| **source == canonical_desktop)
            .count(),
        1,
        "desktop source must be de-duplicated"
    );
    assert_eq!(
        sources
            .iter()
            .filter(|source| **source == canonical_watch)
            .count(),
        1,
        "watch source must be de-duplicated"
    );

    snapshot.watch_paths_draft = SmolStr::new(file.to_string_lossy().as_ref());
    assert!(
        super::validate_settings_sources_for_locale(&snapshot, true)
            .expect_err("file path must be rejected")
            .contains("不是文件夹")
    );
    assert!(
        super::validate_settings_sources_for_locale(&snapshot, false)
            .expect_err("file path must be rejected in English")
            .contains("is not a folder")
    );
    snapshot.watch_paths_draft = SmolStr::new(scratch.join("missing").to_string_lossy().as_ref());
    assert!(
        super::validate_settings_sources_for_locale(&snapshot, true)
            .expect_err("missing path must be rejected")
            .contains("不存在")
    );
    assert!(
        super::validate_settings_sources_for_locale(&snapshot, false)
            .expect_err("missing path must be rejected in English")
            .contains("does not exist")
    );
    assert!(super::settings_path_is_within_prefix(
        r"c:\windows\system32",
        r"c:\windows"
    ));
    assert!(!super::settings_path_is_within_prefix(
        r"c:\windowsold",
        r"c:\windows"
    ));

    let _ = std::fs::remove_dir_all(scratch);
}
