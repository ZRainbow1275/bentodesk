#[test]
fn workspace_aux_surfaces_replace_superseded_tools_but_keep_shell_chrome() {
    assert!(is_workspace_aux_surface(WindowKind::Suggestor));
    assert!(is_workspace_aux_surface(WindowKind::ZoneEditor));
    assert!(is_workspace_aux_surface(WindowKind::SnapshotPicker));
    assert!(is_workspace_aux_surface(WindowKind::BulkManager));
    assert!(!is_workspace_aux_surface(WindowKind::IconPicker));
    assert!(!is_workspace_aux_surface(WindowKind::Settings));

    assert!(hides_when_workspace_aux_opens(WindowKind::Suggestor));
    assert!(hides_when_workspace_aux_opens(WindowKind::IconPicker));
    assert!(!hides_when_workspace_aux_opens(WindowKind::Main));
    assert!(!hides_when_workspace_aux_opens(WindowKind::MiniBar));
    assert!(!hides_when_workspace_aux_opens(WindowKind::Settings));
}

#[test]
fn smart_group_target_and_background_layout_never_reuse_the_zero_origin_card() {
    let mut empty = AppState::new();
    empty.viewport = Size {
        width: 800.0,
        height: 600.0,
    };
    assert_eq!(smart_group_zone_dimensions(800.0, 600.0), (320, 270));
    let target_id = ensure_suggestor_target_zone(&mut empty);
    let target = empty
        .zones
        .get(target_id)
        .expect("empty layout creates target");
    assert_eq!((target.w, target.h), (320, 270));
    assert_eq!(empty.selected_zone.get(), Some(target_id));

    let mut app = AppState::new();
    app.viewport = Size {
        width: 1600.0,
        height: 900.0,
    };
    let ids: Vec<_> = (1..=5).map(ZoneId).collect();
    for id in &ids {
        app.zones.add(Zone::new(*id, "Generated", 0, 0, 200, 120));
    }

    assert_eq!(layout_new_auto_group_zones(&mut app, &ids), 5);
    let mut origins: Vec<_> = ids
        .iter()
        .map(|id| {
            let zone = app.zones.get(*id).expect("created group");
            assert_eq!((zone.w, zone.h), (400, 405));
            (zone.x, zone.y)
        })
        .collect();
    origins.sort_unstable();
    origins.dedup();
    assert_eq!(origins.len(), ids.len());
}

#[test]
fn r13_04_startup_icon_rehydrate_skips_builtin_and_repairs_missing_cache() {
    let shortcut = "C:\\Desktop\\Game.lnk";
    let internet_shortcut = "C:\\Desktop\\Game.url";
    let current_shortcut_hash = bentodesk_backend::icon::protocol::icon_cache_key(shortcut);
    let current_internet_shortcut_hash =
        bentodesk_backend::icon::protocol::icon_cache_key(internet_shortcut);
    assert_eq!(
        item_icon_startup_rehydrate_force(shortcut, "builtin:folder", false),
        None
    );
    assert_eq!(
        item_icon_startup_rehydrate_force("C:\\Desktop\\file.txt", "0123456789abcdef", true),
        None
    );
    assert_eq!(
        item_icon_startup_rehydrate_force("C:\\Desktop\\file.txt", "0123456789abcdef", false),
        Some(false)
    );
    assert_eq!(
        item_icon_startup_rehydrate_force("C:\\Desktop\\file.txt", "", false),
        Some(false)
    );
    assert_eq!(
        item_icon_startup_rehydrate_force(shortcut, "legacy-target-hash", true),
        Some(true),
        "cached target-keyed shortcut icons require one forced migration"
    );
    assert_eq!(
        item_icon_startup_rehydrate_force(shortcut, &current_shortcut_hash, true),
        None
    );
    assert_eq!(
        item_icon_startup_rehydrate_force(
            internet_shortcut,
            &bentodesk_backend::icon::extractor::compute_icon_hash(internet_shortcut),
            true,
        ),
        Some(true),
        "cached generic .url icons require one forced resource migration"
    );
    assert_eq!(
        item_icon_startup_rehydrate_force(internet_shortcut, &current_internet_shortcut_hash, true,),
        None
    );
}

#[test]
fn panel_header_button_hover_maps_shell_hits() {
    assert_eq!(
        panel_header_button_hover_for_hit(Some((ZoneId(9), ui::HeaderButton::Search))),
        Some(PanelHeaderButtonHover::new(
            ZoneId(9),
            PanelHeaderButtonKind::Search
        ))
    );
    assert_eq!(
        panel_header_button_hover_for_hit(Some((ZoneId(9), ui::HeaderButton::Close))),
        Some(PanelHeaderButtonHover::new(
            ZoneId(9),
            PanelHeaderButtonKind::Close
        ))
    );
    assert_eq!(panel_header_button_hover_for_hit(None), None);
}

#[test]
fn settings_encryption_mode_hover_maps_settings_hits() {
    assert_eq!(
        settings_encryption_mode_hover_for_hit(ui::SettingsHit::SelectEncryptionModeNone),
        Some(SettingsEncryptionMode::None)
    );
    assert_eq!(
        settings_encryption_mode_hover_for_hit(ui::SettingsHit::SelectEncryptionModeDpapi),
        Some(SettingsEncryptionMode::Dpapi)
    );
    assert_eq!(
        settings_encryption_mode_hover_for_hit(ui::SettingsHit::SelectEncryptionModePassphrase),
        Some(SettingsEncryptionMode::Passphrase)
    );
    assert_eq!(
        settings_encryption_mode_hover_for_hit(ui::SettingsHit::Body),
        None
    );
}

#[test]
fn settings_appearance_hover_maps_settings_hits() {
    assert_eq!(
        settings_appearance_hover_for_hit(ui::SettingsHit::SelectTheme(5)),
        Some(bentodesk_app::theme_picker::AppearanceHit::Card(5))
    );
    assert_eq!(
        settings_appearance_hover_for_hit(ui::SettingsHit::SelectAccent(3)),
        Some(bentodesk_app::theme_picker::AppearanceHit::Accent(3))
    );
    assert_eq!(
        settings_appearance_hover_for_hit(ui::SettingsHit::EditAccentColor),
        Some(bentodesk_app::theme_picker::AppearanceHit::AccentEditor)
    );
    assert_eq!(
        settings_appearance_hover_for_hit(ui::SettingsHit::OpenAccentColorPicker),
        Some(bentodesk_app::theme_picker::AppearanceHit::AccentPicker)
    );
    assert_eq!(
        settings_appearance_hover_for_hit(ui::SettingsHit::ClearAccentColor),
        Some(bentodesk_app::theme_picker::AppearanceHit::AccentClear)
    );
    assert_eq!(
        settings_appearance_hover_for_hit(ui::SettingsHit::SelectEncryptionModeDpapi),
        None
    );
    assert_eq!(
        settings_appearance_hover_for_hit(ui::SettingsHit::Body),
        None
    );
}

#[test]
fn settings_close_hover_maps_only_close_hit() {
    assert!(settings_close_hover_for_hit(ui::SettingsHit::Close));
    assert!(!settings_close_hover_for_hit(ui::SettingsHit::Body));
    assert!(!settings_close_hover_for_hit(ui::SettingsHit::SelectTheme(
        5
    )));
}

/// A3 (2026-05-29) — the former 80ms `LEAVE_GRACE_MS` dead stub is now
/// LIVE: the hover-intent expand and grace-collapse are driven by the
/// user-tunable `expand_delay_ms` / `collapse_delay_ms` settings through
/// `drive_hover_scheduler` + `poll_hover_scheduler`. This test exercises
/// the full shell wiring (enter → deferred expand; leave → deferred
/// collapse) using frame-tick timestamps, the no-live-mouse substitute.
#[test]
fn hover_scheduler_defers_expand_and_collapse_via_settings_delays() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        // Default ZoneDisplayMode::Hover so the leave auto-collapses.
        app.zones
            .add(Zone::new(ZoneId(1), "Compiler", 100, 100, 240, 180));
        app.expand_delay_ms.set(DEFAULT_EXPAND_DELAY_MS);
        app.collapse_delay_ms.set(DEFAULT_COLLAPSE_DELAY_MS);
    }
    let app = root.app.borrow();
    let expand_delay = app.expand_delay_ms.get() as u32;
    let collapse_delay = app.collapse_delay_ms.get() as u32;

    // 1. Cursor enters the pill at t=1000 — expand is ARMED, not fired.
    drive_hover_scheduler(&app, Some(ZoneId(1)), 1_000);
    assert!(app.hover_scheduler.get().is_pending());
    assert_eq!(app.hover_scheduler.get().expanded_zone(), None);
    // Polling before the delay does nothing.
    assert!(!poll_hover_scheduler(&app, 1_000 + expand_delay - 1));
    assert_eq!(app.zone_pill_anim_zone.get(), None);

    // 2. At now + expand_delay the morph flips to expanding.
    assert!(poll_hover_scheduler(&app, 1_000 + expand_delay));
    assert_eq!(app.hover_scheduler.get().expanded_zone(), Some(ZoneId(1)));
    assert_eq!(app.zone_pill_anim_zone.get(), Some(ZoneId(1)));
    assert!(app.zone_pill_anim_expanding.get());

    // 3. Cursor leaves to empty space after the shared morph-derived
    //    expand-lock; collapse is ARMED.
    let leave = 1_000 + expand_delay + zone_pill_geometry::EXPAND_LOCK_MS + 10;
    drive_hover_scheduler(&app, None, leave);
    assert!(app.hover_scheduler.get().is_pending());
    // Before now + collapse_delay the morph is still expanding.
    assert!(!poll_hover_scheduler(&app, leave + collapse_delay - 1));
    assert!(app.zone_pill_anim_expanding.get());

    // 4. At now + collapse_delay the collapse morph fires.
    assert!(poll_hover_scheduler(&app, leave + collapse_delay));
    assert!(!app.zone_pill_anim_expanding.get());
    assert_eq!(app.hover_scheduler.get().expanded_zone(), None);
}

/// A3/V21-A1: keep the on-demand hover frame timer alive while the
/// scheduler is pending or a capsule morph is still in flight.
#[test]
fn hover_frame_pump_needed_tracks_pending_scheduler_and_morph() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(1), "Compiler", 100, 100, 240, 180));
        app.expand_delay_ms.set(DEFAULT_EXPAND_DELAY_MS);
        app.collapse_delay_ms.set(DEFAULT_COLLAPSE_DELAY_MS);
    }
    let app = root.app.borrow();
    assert!(!hover_frame_pump_needed(&app));

    drive_hover_scheduler(&app, Some(ZoneId(1)), 1_000);
    assert!(hover_frame_pump_needed(&app));

    let expand_at = 1_000 + DEFAULT_EXPAND_DELAY_MS as u32;
    assert!(poll_hover_scheduler(&app, expand_at));
    assert!(hover_frame_pump_needed(&app));
    assert!(tick_zone_pill_animation(
        &app,
        expand_at + zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS
    ));
    assert!(!hover_frame_pump_needed(&app));

    drive_hover_scheduler(
        &app,
        None,
        expand_at + zone_pill_geometry::EXPAND_LOCK_MS + 10,
    );
    assert!(hover_frame_pump_needed(&app));

    assert!(poll_hover_scheduler(
        &app,
        expand_at + zone_pill_geometry::EXPAND_LOCK_MS + 10 + DEFAULT_COLLAPSE_DELAY_MS as u32
    ));
    assert!(hover_frame_pump_needed(&app));
    assert!(tick_zone_pill_animation(
        &app,
        expand_at
            + zone_pill_geometry::EXPAND_LOCK_MS
            + 10
            + DEFAULT_COLLAPSE_DELAY_MS as u32
            + zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS
    ));
    assert!(!hover_frame_pump_needed(&app));
}

#[test]
fn hover_frame_pump_needed_tracks_theme_transition() {
    let root = test_app_root();
    let app = root.app.borrow();
    // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
    let now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
    app.settings_open.set(true);
    let from_card = app.active_theme_card_id();

    assert_eq!(app.apply_active_theme_by_id("light"), Some(true));
    assert!(app.start_theme_transition_from(from_card, now_ms));
    assert!(hover_frame_pump_needed(&app));

    app.theme_transition_started_ms
        .set(now_ms.wrapping_sub(bentodesk_app::state::THEME_TRANSITION_MS));
    assert!(!hover_frame_pump_needed(&app));

    app.settings_open.set(false);
    assert!(!hover_frame_pump_needed(&app));
}

#[test]
fn hover_frame_pump_needed_tracks_settings_open_animation() {
    let root = test_app_root();
    let app = root.app.borrow();
    // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
    let now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };

    app.settings_open.set(true);
    app.start_settings_open_animation(now_ms);
    assert!(hover_frame_pump_needed(&app));

    app.settings_open_started_ms
        .set(now_ms.wrapping_sub(bentodesk_app::state::SETTINGS_OPEN_ANIMATION_MS));
    assert!(!hover_frame_pump_needed(&app));
}

/// 07-22 hand-test: structural hover belongs exclusively to Hover mode.
/// Always is already open from its steady-state predicate and Click waits
/// for an explicit click; neither may arm a delayed hover morph.
#[test]
fn hover_scheduler_does_not_arm_for_always_or_click_modes() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(1), "Pinned", 100, 100, 240, 180));
        app.expand_delay_ms.set(DEFAULT_EXPAND_DELAY_MS);
        app.collapse_delay_ms.set(DEFAULT_COLLAPSE_DELAY_MS);
    }
    let app = root.app.borrow();
    for mode in [ZoneDisplayMode::Always, ZoneDisplayMode::Click] {
        app.set_zone_display_mode(mode);
        drive_hover_scheduler(&app, Some(ZoneId(1)), 1_000);
        assert!(!app.hover_scheduler.get().is_pending(), "mode={mode:?}");
        assert!(!poll_hover_scheduler(
            &app,
            1_000 + DEFAULT_EXPAND_DELAY_MS as u32
        ));
        assert_eq!(app.hover_scheduler.get().expanded_zone(), None);
    }
}

#[test]
fn startup_diag_skip_value_matches_tokens_case_insensitively() {
    assert!(!startup_diag_skip_value(None, "ghost"));
    assert!(!startup_diag_skip_value(Some("icon,rules"), "ghost"));
    assert!(startup_diag_skip_value(Some(" icon, GHOST "), "ghost"));
    assert!(startup_diag_skip_value(Some("all"), "desktop_watcher"));
}

#[test]
fn win32_timer_ids_are_unique() {
    let timer_ids = [
        TRAY_ICON_RETRY_TIMER_ID,
        GHOST_PASSTHROUGH_TIMER_ID,
        BACKEND_EVENT_POLL_TIMER_ID,
        HOVER_FRAME_TIMER_ID,
        STARTUP_MEMORY_TRIM_TIMER_ID,
        RESIDENT_MEMORY_TRIM_TIMER_ID,
        STACK_TRAY_MEMORY_TRIM_TIMER_ID,
        CONTEXT_MENU_INPUT_TIMER_ID,
    ];
    for (index, id) in timer_ids.iter().enumerate() {
        assert!(
            !timer_ids[index + 1..].contains(id),
            "timer id must not be reused: {id:#X}",
        );
    }
}

#[test]
fn hover_frame_timer_requests_one_windows_timer_quantum() {
    assert_eq!(HOVER_FRAME_POLL_MS, 10);
}

#[test]
fn context_menu_mouse_poll_accepts_down_and_since_last_call_bits() {
    assert!(async_mouse_button_active(i16::MIN));
    assert!(async_mouse_button_active(0x0001));
    assert!(async_mouse_button_active(i16::MIN | 0x0001));
    assert!(!async_mouse_button_active(0));
}

#[test]
fn covered_main_window_clears_stale_hover_without_interrupting_drag() {
    let main = 1usize as HWND;
    let covering_window = 2usize as HWND;

    assert!(super::should_clear_stale_main_hover(
        true,
        false,
        covering_window,
        main
    ));
    assert!(!super::should_clear_stale_main_hover(
        false,
        false,
        covering_window,
        main
    ));
    assert!(!super::should_clear_stale_main_hover(
        true,
        true,
        covering_window,
        main
    ));
    assert!(!super::should_clear_stale_main_hover(
        true, false, main, main
    ));
}

#[test]
fn desktop_source_rows_mark_every_resolved_source_watched() {
    let dirs = [
        std::path::PathBuf::from(r"C:\Users\Public\Desktop"),
        std::path::PathBuf::from(r"D:\Desktop"),
    ];

    let rows = desktop_source_rows_for_settings(&dirs);

    assert_eq!(rows.len(), dirs.len());
    assert!(rows.iter().all(|(_, _, watched)| *watched));
    assert_eq!(
        rows[0].0,
        bentodesk_backend::desktop_sources::DesktopSourceKind::Public
    );
    assert_eq!(rows[1].1.as_str(), r"D:\Desktop");
}

#[test]
fn widen_static_nul_terminates_short_input() {
    let buf = widen_static::<32>("Hi");
    assert_eq!(buf[0], b'H' as u16);
    assert_eq!(buf[1], b'i' as u16);
    assert_eq!(buf[2], 0, "must NUL-terminate after the last char");
    assert!(buf[3..].iter().all(|u| *u == 0));
}

#[test]
fn widen_static_truncates_overflow_and_keeps_nul() {
    let buf = widen_static::<4>("ABCDE");
    assert_eq!(buf[0], b'A' as u16);
    assert_eq!(buf[1], b'B' as u16);
    assert_eq!(buf[2], b'C' as u16);
    assert_eq!(buf[3], 0, "last slot must always be NUL");
}

#[test]
fn widen_static_handles_cjk_characters() {
    let buf = widen_static::<8>("\u{9000}\u{51fa}");
    assert_eq!(buf[0], 0x9000);
    assert_eq!(buf[1], 0x51FA);
    assert_eq!(buf[2], 0);
}

#[test]
fn widen_static_zero_capacity_returns_empty_array() {
    let buf = widen_static::<0>("anything");
    assert_eq!(buf.len(), 0);
}

#[test]
fn tray_menu_item_mapping_covers_one_x_actions() {
    let origin = DispatchPoint::new(640, 360);

    assert!(matches!(
        tray_menu_command_for_item(TrayMenuItem::ShowHideMain, true, origin),
        Command::HideWindow(WindowKind::Main)
    ));
    assert!(matches!(
        tray_menu_command_for_item(TrayMenuItem::ShowHideMain, false, origin),
        Command::ShowWindow(WindowKind::Main)
    ));
    match tray_menu_command_for_item(TrayMenuItem::NewZone, true, origin) {
        Command::CreateZone(spec) => {
            assert_eq!(spec.name.as_str(), "Zone");
            assert_eq!(spec.origin, origin);
            assert_eq!(spec.size, DispatchSize::new(200, 120));
        }
        other => panic!("expected tray NewZone to create zone, got {other:?}"),
    }
    assert!(matches!(
        tray_menu_command_for_item(TrayMenuItem::AutoOrganize, true, origin),
        Command::AutoOrganize
    ));
    assert!(matches!(
        tray_menu_command_for_item(TrayMenuItem::OpenSettings, true, origin),
        Command::OpenSettings
    ));
    assert!(matches!(
        tray_menu_command_for_item(TrayMenuItem::About, true, origin),
        Command::OpenAbout
    ));
    assert!(matches!(
        tray_menu_command_for_item(TrayMenuItem::Exit, true, origin),
        Command::QuitApp
    ));
}

#[test]
fn tray_menu_choice_mapping_preserves_dismiss_and_one_based_ids() {
    let origin = DispatchPoint::new(640, 360);

    assert!(tray_menu_command_for_choice(0, true, origin).is_none());
    assert!(tray_menu_command_for_choice(-1, true, origin).is_none());
    assert!(tray_menu_command_for_choice(999, true, origin).is_none());

    assert!(matches!(
        tray_menu_command_for_choice(1, true, origin),
        Some(Command::HideWindow(WindowKind::Main))
    ));
    assert!(matches!(
        tray_menu_command_for_choice(1, false, origin),
        Some(Command::ShowWindow(WindowKind::Main))
    ));
    match tray_menu_command_for_choice(2, true, origin) {
        Some(Command::CreateZone(spec)) => {
            assert_eq!(spec.name.as_str(), "Zone");
            assert_eq!(spec.origin, origin);
            assert_eq!(spec.size, DispatchSize::new(200, 120));
        }
        other => panic!("expected tray choice 2 to create zone, got {other:?}"),
    }
    assert!(matches!(
        tray_menu_command_for_choice(3, true, origin),
        Some(Command::AutoOrganize)
    ));
    assert!(matches!(
        tray_menu_command_for_choice(4, true, origin),
        Some(Command::OpenSettings)
    ));
    assert!(matches!(
        tray_menu_command_for_choice(5, true, origin),
        Some(Command::OpenAbout)
    ));
    assert!(matches!(
        tray_menu_command_for_choice(6, true, origin),
        Some(Command::QuitApp)
    ));
}

#[test]
fn tray_notify_icon_data_uses_stable_shell_contract() {
    let hwnd = 0x1234usize as windows_sys::Win32::Foundation::HWND;
    let icon = 0x5678usize as windows_sys::Win32::UI::WindowsAndMessaging::HICON;

    let nid = build_tray_notify_icon_data(hwnd, icon, false);
    let expected_tip: Vec<u16> = "BentoDesk".encode_utf16().collect();

    assert_eq!(nid.cbSize, core::mem::size_of::<NOTIFYICONDATAW>() as u32);
    assert_eq!(nid.hWnd, hwnd);
    assert_eq!(nid.uID, super::TRAY_ICON_ID);
    assert_eq!(nid.guidItem.data1, super::TRAY_ICON_GUID.data1);
    assert_eq!(nid.guidItem.data2, super::TRAY_ICON_GUID.data2);
    assert_eq!(nid.guidItem.data3, super::TRAY_ICON_GUID.data3);
    assert_eq!(nid.guidItem.data4, super::TRAY_ICON_GUID.data4);
    assert_eq!(nid.uCallbackMessage, super::WM_TRAY_ICON);
    assert_eq!(nid.hIcon, icon);
    assert_eq!(nid.uFlags & NIF_GUID, NIF_GUID);
    assert_eq!(nid.uFlags & NIF_ICON, NIF_ICON);
    assert_eq!(nid.uFlags & NIF_MESSAGE, NIF_MESSAGE);
    assert_eq!(nid.uFlags & NIF_SHOWTIP, NIF_SHOWTIP);
    assert_eq!(nid.uFlags & NIF_TIP, NIF_TIP);
    assert_eq!(&nid.szTip[..expected_tip.len()], expected_tip.as_slice());
    assert_eq!(nid.szTip[expected_tip.len()], 0);
}

#[test]
fn tray_delete_icon_data_uses_same_guid_identity() {
    let hwnd = 0x1234usize as windows_sys::Win32::Foundation::HWND;

    let nid = build_tray_delete_icon_data(hwnd, false);

    assert_eq!(nid.cbSize, core::mem::size_of::<NOTIFYICONDATAW>() as u32);
    assert_eq!(nid.hWnd, hwnd);
    assert_eq!(nid.uID, super::TRAY_ICON_ID);
    assert_eq!(nid.uFlags, NIF_GUID);
    assert_eq!(nid.guidItem.data1, super::TRAY_ICON_GUID.data1);
    assert_eq!(nid.guidItem.data2, super::TRAY_ICON_GUID.data2);
    assert_eq!(nid.guidItem.data3, super::TRAY_ICON_GUID.data3);
    assert_eq!(nid.guidItem.data4, super::TRAY_ICON_GUID.data4);
}

#[test]
fn tray_uid_only_fallback_drops_guid_identity() {
    // Mc-3 #15 — the relocated-portable-install fallback registers under
    // the path-independent (hWnd, uID) identity: no NIF_GUID, zeroed guid.
    let hwnd = 0x1234usize as windows_sys::Win32::Foundation::HWND;
    let icon = 0x5678usize as windows_sys::Win32::UI::WindowsAndMessaging::HICON;

    let nid = build_tray_notify_icon_data(hwnd, icon, true);
    assert_eq!(nid.hWnd, hwnd);
    assert_eq!(nid.uID, super::TRAY_ICON_ID);
    assert_eq!(nid.hIcon, icon);
    assert_eq!(nid.uCallbackMessage, super::WM_TRAY_ICON);
    // The defining property of the fallback: GUID identity is fully dropped.
    assert_eq!(nid.uFlags & NIF_GUID, 0);
    assert_eq!(nid.guidItem.data1, 0);
    assert_eq!(nid.guidItem.data2, 0);
    assert_eq!(nid.guidItem.data3, 0);
    // Functional flags are still present so the icon/menu still work.
    assert_eq!(nid.uFlags & NIF_ICON, NIF_ICON);
    assert_eq!(nid.uFlags & NIF_MESSAGE, NIF_MESSAGE);
    assert_eq!(nid.uFlags & NIF_SHOWTIP, NIF_SHOWTIP);
    assert_eq!(nid.uFlags & NIF_TIP, NIF_TIP);

    let del = build_tray_delete_icon_data(hwnd, true);
    assert_eq!(del.hWnd, hwnd);
    assert_eq!(del.uID, super::TRAY_ICON_ID);
    assert_eq!(del.uFlags & NIF_GUID, 0);
    assert_eq!(del.guidItem.data1, 0);
    assert_eq!(del.guidItem.data2, 0);
    assert_eq!(del.guidItem.data3, 0);
}
