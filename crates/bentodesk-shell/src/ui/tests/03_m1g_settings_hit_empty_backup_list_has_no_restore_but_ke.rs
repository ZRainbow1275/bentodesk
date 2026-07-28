/// M1g — empty list: with no backup entries there is no per-row restore
/// hit (the empty-placeholder row is non-interactive), while 立即备份 stays
/// reachable.
#[test]
fn m1g_settings_hit_empty_backup_list_has_no_restore_but_keeps_create() {
    let app = app_with_zones(vec![]);
    assert!(app.settings_backup_entries.borrow().is_empty());

    let flags = bentodesk_app::settings_panel::SettingsBodyFlags::new(
        app.crash_restart_enabled.get(),
        app.safe_start_after_hibernation.get(),
        false,
        false,
        bentodesk_app::settings_panel::UpdaterHeightKind::StatusOnly,
    )
    .with_backup_rows(0);
    // M5 cleanup — scroll the Backup card to the top of the visible body
    // (it is no longer the bottom of the content; see the sibling
    // reachability test for why).
    let reserve_delta_0 =
        bentodesk_app::settings_panel::settings_sources_reserve_delta(flags.source_row_count);
    let label_unscrolled = bentodesk_app::settings_panel::settings_backup_label_rect(
        app.viewport,
        reserve_delta_0,
        &flags,
    )
    .y;
    let scroll_off = scroll_offset_to_top_of_body(app.viewport, &flags, label_unscrolled);
    app.scroll_offset_y.set(scroll_off);
    // M1i fidelity — `settings_hit` folds the §2 source reserve delta into
    // the scroll for all perf-and-below geometry; this test populates no
    // desktop sources (count 0), so apply the matching fold to the rects we
    // compare against, exactly as production paint/hit does.
    let scroll_y = scroll_off + reserve_delta_0;
    let actions = bentodesk_app::settings_panel::settings_backup_actions_row_rect(
        app.viewport,
        scroll_y,
        &flags,
    );
    let create = bentodesk_app::settings_panel::settings_backup_create_button_rect(actions);
    assert_eq!(
        settings_hit(
            &app,
            create.x + create.width * 0.5,
            create.y + create.height * 0.5
        ),
        SettingsHit::CreateSettingsBackup,
    );
    // The empty-placeholder row's centre must NOT produce a restore hit —
    // it eats as Body (non-interactive).
    let empty_row = bentodesk_app::settings_panel::settings_backup_entry_row_rect(
        app.viewport,
        scroll_y,
        &flags,
        0,
    );
    let hit = settings_hit(
        &app,
        empty_row.x + empty_row.width * 0.5,
        empty_row.y + empty_row.height * 0.5,
    );
    assert_ne!(hit, SettingsHit::RestoreSettingsBackup(0));
}

/// α4 (Wave I-α, 2026-05-25) / G3 parity (2026-06-01) — clicking each of the
/// three zone-display radio hit-boxes routes to the matching
/// `SetZoneDisplayMode(mode)` variant. Each hit-box centre is sampled so the
/// test exercises the hit-tester (not the geometry, which has its own
/// settings_panel.rs tests).
///
/// G3 reorder: the §4 DisplayMode picker was promoted out of the General
/// band to sit between §3 Appearance and §5 Performance, so at scroll 0 it
/// is below the visible body fold. Scroll the §4 group title to the body top
/// first (same scaffold the backup/plugins reachability tests use), then
/// sample the radio centres at the production-folded scroll_y.
#[test]
fn alpha4_three_radio_hit_boxes_route_to_set_zone_display_mode() {
    let app = app_with_zones(vec![]);
    // Default flag set (no desktop sources populated → source_row_count 0).
    let flags = bentodesk_app::settings_panel::SettingsBodyFlags::new(
        app.crash_restart_enabled.get(),
        app.safe_start_after_hibernation.get(),
        false,
        false,
        bentodesk_app::settings_panel::UpdaterHeightKind::StatusOnly,
    );
    // Scroll the §4 DisplayMode group title to the top of the visible body.
    // The picker geometry roots at the FIXED source reserve baseline (like
    // Appearance/Performance), so fold the §2 reserve delta into scroll the
    // same way production paint/hit does.
    let reserve_delta_0 =
        bentodesk_app::settings_panel::settings_sources_reserve_delta(flags.source_row_count);
    let label_unscrolled = bentodesk_app::settings_panel::settings_display_mode_label_rect(
        app.viewport,
        reserve_delta_0,
    )
    .y;
    let scroll_off = scroll_offset_to_top_of_body(app.viewport, &flags, label_unscrolled);
    app.scroll_offset_y.set(scroll_off);
    let scroll_y = scroll_off + reserve_delta_0;

    // Sanity: the §4 picker row must now be inside the visible body.
    let body = bentodesk_app::settings_panel::settings_body_rect(app.viewport);
    let picker = bentodesk_app::settings_panel::settings_zone_display_mode_picker_row_rect(
        app.viewport,
        scroll_y,
    );
    assert!(
        picker.y >= body.y && picker.y < body.bottom(),
        "§4 DisplayMode picker must scroll into the visible body \
         (picker.y={}, body=[{}, {}])",
        picker.y,
        body.y,
        body.bottom(),
    );

    let r_hover = bentodesk_app::settings_panel::settings_zone_display_mode_radio_rect(
        app.viewport,
        scroll_y,
        0,
    );
    assert_eq!(
        settings_hit(
            &app,
            r_hover.x + r_hover.width * 0.5,
            r_hover.y + r_hover.height * 0.5,
        ),
        SettingsHit::SetZoneDisplayMode(bentodesk_app::ZoneDisplayMode::Hover)
    );

    let r_always = bentodesk_app::settings_panel::settings_zone_display_mode_radio_rect(
        app.viewport,
        scroll_y,
        1,
    );
    assert_eq!(
        settings_hit(
            &app,
            r_always.x + r_always.width * 0.5,
            r_always.y + r_always.height * 0.5,
        ),
        SettingsHit::SetZoneDisplayMode(bentodesk_app::ZoneDisplayMode::Always)
    );

    let r_click = bentodesk_app::settings_panel::settings_zone_display_mode_radio_rect(
        app.viewport,
        scroll_y,
        2,
    );
    assert_eq!(
        settings_hit(
            &app,
            r_click.x + r_click.width * 0.5,
            r_click.y + r_click.height * 0.5,
        ),
        SettingsHit::SetZoneDisplayMode(bentodesk_app::ZoneDisplayMode::Click)
    );
}

#[test]
fn settings_hit_routes_keybindings_modal_buttons_first() {
    let app = app_with_zones(vec![]);
    app.settings_keybindings_open.set(true);

    let record = settings_keybinding_record_rect(app.viewport, 0);
    assert_eq!(
        settings_hit(
            &app,
            record.x + record.width * 0.5,
            record.y + record.height * 0.5
        ),
        SettingsHit::RecordKeybinding(0)
    );

    let reset = settings_keybinding_reset_rect(app.viewport, 1);
    assert_eq!(
        settings_hit(
            &app,
            reset.x + reset.width * 0.5,
            reset.y + reset.height * 0.5
        ),
        SettingsHit::ResetKeybinding(1)
    );

    let close = settings_keybindings_close_rect(app.viewport);
    assert_eq!(
        settings_hit(
            &app,
            close.x + close.width * 0.5,
            close.y + close.height * 0.5
        ),
        SettingsHit::CloseKeybindings
    );

    let modal = settings_keybindings_modal_rect(app.viewport);
    assert_eq!(
        settings_hit(&app, modal.x + 8.0, modal.y + modal.height - 8.0),
        SettingsHit::Body
    );
}

/// M1h — reachability: with plugin entries seeded, clicking the full-width
/// 安装插件... button / per-card enable toggle / per-card 卸载 resolves to
/// `InstallPlugin` / `TogglePlugin(idx)` / `UninstallPlugin(idx)` against
/// the INLINE §11 geometry (no modal). Proves the paint→hit chain is wired
/// after the modal→inline move — no plugin control is painted-but-unwired.
/// Builds the SAME `SettingsBodyFlags` (idle updater + empty backup +
/// capped plugin count) the hit-tester derives so the sampled centres line
/// up with production geometry, then scrolls the bottom Plugins section
/// into the visible body.
#[test]
fn m1h_settings_hit_resolves_inline_plugin_install_toggle_and_per_card_uninstall() {
    let app = app_with_zones(vec![]);
    // Seed two real-shaped entries so the per-card toggle/uninstall paths
    // are live (different kinds + enabled states for good measure).
    app.settings_plugin_entries.replace(vec![
        SettingsPluginEntry {
            id: smol_str::SmolStr::new_static("com.test.theme"),
            name: smol_str::SmolStr::new_static("Theme"),
            version: smol_str::SmolStr::new_static("1.0.0"),
            plugin_type: smol_str::SmolStr::new_static("theme"),
            author: smol_str::SmolStr::new_static("Acme"),
            description: smol_str::SmolStr::new_static("A theme plugin"),
            enabled: true,
        },
        SettingsPluginEntry {
            id: smol_str::SmolStr::new_static("com.test.widget"),
            name: smol_str::SmolStr::new_static("Widget"),
            version: smol_str::SmolStr::new_static("2.0.0"),
            plugin_type: smol_str::SmolStr::new_static("widget"),
            author: smol_str::SmolStr::new_static("Acme"),
            description: smol_str::SmolStr::new_static("A widget plugin"),
            enabled: false,
        },
    ]);

    // Rebuild the EXACT flags the hit-tester derives: live Startup gating
    // bools + idle updater + empty backup list + capped visible plugin
    // count. Reading them off `app` keeps the sampled rects production-true.
    let entries = app.settings_plugin_entries.borrow();
    let visible =
        bentodesk_app::business::settings::plugins_section::plugin_visible_row_count(&entries);
    let flags = bentodesk_app::settings_panel::SettingsBodyFlags::new(
        app.crash_restart_enabled.get(),
        app.safe_start_after_hibernation.get(),
        false,
        false,
        bentodesk_app::settings_panel::UpdaterHeightKind::StatusOnly,
    )
    .with_backup_rows(0)
    .with_plugin_rows(visible);
    drop(entries);

    // M5 cleanup — Plugins §11 is no longer LAST in the body (the M6-UI §3
    // Appearance grid was appended below it), so scrolling to `max_scroll`
    // reveals the trailing Appearance grid, not Plugins. Scroll precisely so
    // the Plugins label sits at the top of the visible body instead.
    let reserve_delta_0 =
        bentodesk_app::settings_panel::settings_sources_reserve_delta(flags.source_row_count);
    let label_unscrolled = bentodesk_app::settings_panel::settings_plugins_label_rect(
        app.viewport,
        reserve_delta_0,
        &flags,
    )
    .y;
    let scroll_off = scroll_offset_to_top_of_body(app.viewport, &flags, label_unscrolled);
    app.scroll_offset_y.set(scroll_off);
    // M1i fidelity — `settings_hit` folds the §2 source reserve delta into
    // the scroll for all perf-and-below geometry; this test populates no
    // desktop sources (count 0), so apply the matching fold to the rects we
    // compare against, exactly as production paint/hit does.
    let scroll_y = scroll_off + reserve_delta_0;
    let body = bentodesk_app::settings_panel::settings_body_rect(app.viewport);
    let label =
        bentodesk_app::settings_panel::settings_plugins_label_rect(app.viewport, scroll_y, &flags);
    assert!(
        label.y >= body.y && label.y < body.bottom(),
        "plugins section must scroll into the visible body (label.y={}, body=[{}, {}])",
        label.y,
        body.y,
        body.bottom(),
    );

    // 安装插件... full-width button → InstallPlugin.
    let install = bentodesk_app::settings_panel::settings_plugins_install_button_rect(
        app.viewport,
        scroll_y,
        &flags,
    );
    assert_eq!(
        settings_hit(
            &app,
            install.x + install.width * 0.5,
            install.y + install.height * 0.5
        ),
        SettingsHit::InstallPlugin,
    );

    // Per-card enable toggle + 卸载 — each routes to its own card index.
    for card_index in 0..visible {
        let card = bentodesk_app::settings_panel::settings_plugin_card_rect(
            app.viewport,
            scroll_y,
            &flags,
            card_index,
        );
        let toggle = bentodesk_app::settings_panel::settings_plugin_toggle_hit_rect(card);
        assert_eq!(
            settings_hit(
                &app,
                toggle.x + toggle.width * 0.5,
                toggle.y + toggle.height * 0.5
            ),
            SettingsHit::TogglePlugin(card_index),
            "per-card toggle must carry the list index",
        );
        let uninstall = bentodesk_app::settings_panel::settings_plugin_uninstall_button_rect(card);
        assert_eq!(
            settings_hit(
                &app,
                uninstall.x + uninstall.width * 0.5,
                uninstall.y + uninstall.height * 0.5,
            ),
            SettingsHit::UninstallPlugin(card_index),
            "per-card uninstall must carry the list index",
        );
    }

    // Destructive removal is two-step: once armed, the same right action
    // becomes Confirm and the adjacent neutral action becomes Cancel.
    app.settings_plugin_uninstall_confirm.set(Some(0));
    let first_card = bentodesk_app::settings_panel::settings_plugin_card_rect(
        app.viewport,
        scroll_y,
        &flags,
        0,
    );
    let confirm = bentodesk_app::settings_panel::settings_plugin_uninstall_button_rect(first_card);
    assert_eq!(
        settings_hit(
            &app,
            confirm.x + confirm.width * 0.5,
            confirm.y + confirm.height * 0.5,
        ),
        SettingsHit::ConfirmUninstallPlugin(0),
    );
    let cancel =
        bentodesk_app::settings_panel::settings_plugin_uninstall_cancel_button_rect(first_card);
    assert_eq!(
        settings_hit(
            &app,
            cancel.x + cancel.width * 0.5,
            cancel.y + cancel.height * 0.5,
        ),
        SettingsHit::CancelUninstallPlugin,
    );
}

/// M1h — empty list: with no plugins there is no per-card toggle/uninstall
/// hit (the empty-placeholder row is non-interactive), but the full-width
/// 安装插件... button stays reachable.
#[test]
fn m1h_settings_hit_empty_plugin_list_keeps_install_but_has_no_card_hits() {
    let app = app_with_zones(vec![]);
    assert!(app.settings_plugin_entries.borrow().is_empty());

    let flags = bentodesk_app::settings_panel::SettingsBodyFlags::new(
        app.crash_restart_enabled.get(),
        app.safe_start_after_hibernation.get(),
        false,
        false,
        bentodesk_app::settings_panel::UpdaterHeightKind::StatusOnly,
    )
    .with_backup_rows(0)
    .with_plugin_rows(0);
    // M5 cleanup — scroll the Plugins card to the top of the visible body
    // (it is no longer the bottom of the content; the §3 Appearance grid
    // flows below it).
    let reserve_delta_0 =
        bentodesk_app::settings_panel::settings_sources_reserve_delta(flags.source_row_count);
    let label_unscrolled = bentodesk_app::settings_panel::settings_plugins_label_rect(
        app.viewport,
        reserve_delta_0,
        &flags,
    )
    .y;
    let scroll_off = scroll_offset_to_top_of_body(app.viewport, &flags, label_unscrolled);
    app.scroll_offset_y.set(scroll_off);
    // M1i fidelity — `settings_hit` folds the §2 source reserve delta into
    // the scroll for all perf-and-below geometry; this test populates no
    // desktop sources (count 0), so apply the matching fold to the rects we
    // compare against, exactly as production paint/hit does.
    let scroll_y = scroll_off + reserve_delta_0;
    let install = bentodesk_app::settings_panel::settings_plugins_install_button_rect(
        app.viewport,
        scroll_y,
        &flags,
    );
    assert_eq!(
        settings_hit(
            &app,
            install.x + install.width * 0.5,
            install.y + install.height * 0.5
        ),
        SettingsHit::InstallPlugin,
    );
    // The empty-placeholder row's centre must NOT produce a plugin hit — it
    // eats as Body (non-interactive).
    let empty_row = bentodesk_app::settings_panel::settings_plugin_empty_row_rect(
        app.viewport,
        scroll_y,
        &flags,
    );
    let empty_hit = settings_hit(
        &app,
        empty_row.x + empty_row.width * 0.5,
        empty_row.y + empty_row.height * 0.5,
    );
    assert_ne!(empty_hit, SettingsHit::TogglePlugin(0));
    assert_ne!(empty_hit, SettingsHit::UninstallPlugin(0));
    assert_ne!(empty_hit, SettingsHit::ConfirmUninstallPlugin(0));
}

// ── M7 — Encryption §10 inline card hit tests ──────────────────────────

/// Helper: scroll the §10 Encryption section to the top of the visible body
/// and return the (viewport-folded) scroll_y the hit-tester sees, plus the
/// `backup_flags`-equivalent flag set the encryption card uses (no variable
/// rows of its own). Mirrors the backup/plugins reachability-test scaffold.
fn scroll_encryption_into_body(
    app: &AppState,
) -> (bentodesk_app::settings_panel::SettingsBodyFlags, f32) {
    let flags = bentodesk_app::settings_panel::SettingsBodyFlags::new(
        app.crash_restart_enabled.get(),
        app.safe_start_after_hibernation.get(),
        false,
        false,
        bentodesk_app::settings_panel::UpdaterHeightKind::StatusOnly,
    )
    .with_backup_rows(0);
    let reserve_delta_0 =
        bentodesk_app::settings_panel::settings_sources_reserve_delta(flags.source_row_count);
    let label_unscrolled = bentodesk_app::settings_panel::settings_encryption_label_rect(
        app.viewport,
        reserve_delta_0,
        &flags,
    )
    .y;
    let scroll_off = scroll_offset_to_top_of_body(app.viewport, &flags, label_unscrolled);
    app.scroll_offset_y.set(scroll_off);
    (flags, scroll_off + reserve_delta_0)
}

/// M7 — reachability: clicking the three §10 mode buttons resolves to
/// `SelectEncryptionModeNone` / `SelectEncryptionModeDpapi` /
/// `SelectEncryptionModePassphrase`. Proves the paint→hit chain is wired
/// after the deferred-card landed — no mode button is painted-but-unwired.
#[test]
fn m7_settings_hit_resolves_three_mode_buttons() {
    let app = app_with_zones(vec![]);
    let (flags, scroll_y) = scroll_encryption_into_body(&app);
    let body = bentodesk_app::settings_panel::settings_body_rect(app.viewport);
    let label = bentodesk_app::settings_panel::settings_encryption_label_rect(
        app.viewport,
        scroll_y,
        &flags,
    );
    assert!(
        label.y >= body.y && label.y < body.bottom(),
        "encryption section must scroll into the visible body (label.y={}, body=[{}, {}])",
        label.y,
        body.y,
        body.bottom(),
    );
    let expected = [
        SettingsHit::SelectEncryptionModeNone,
        SettingsHit::SelectEncryptionModeDpapi,
        SettingsHit::SelectEncryptionModePassphrase,
    ];
    for (index, want) in expected.iter().enumerate() {
        let btn = bentodesk_app::settings_panel::settings_encryption_mode_button_rect(
            app.viewport,
            scroll_y,
            &flags,
            index as u8,
        );
        assert_eq!(
            settings_hit(&app, btn.x + btn.width * 0.5, btn.y + btn.height * 0.5),
            *want,
            "mode button {index} must resolve to its own SettingsHit",
        );
    }
}

/// M7 — reachability: clicking the masked passphrase input box resolves to
/// `FocusPassphraseField`.
#[test]
fn m7_settings_hit_resolves_passphrase_field_focus() {
    let app = app_with_zones(vec![]);
    let (flags, scroll_y) = scroll_encryption_into_body(&app);
    let input = bentodesk_app::settings_panel::settings_encryption_passphrase_input_rect(
        app.viewport,
        scroll_y,
        &flags,
    );
    assert_eq!(
        settings_hit(
            &app,
            input.x + input.width * 0.5,
            input.y + input.height * 0.5
        ),
        SettingsHit::FocusPassphraseField,
    );
}

/// M7 — regression: the §2 桌面路径 / 监控值 input rects still resolve to
/// `EditDesktopPath` / `EditWatchValues` after the §10 card shifted the
/// section offsets below §9 (the §2 fields sit ABOVE the encryption card,
/// so their geometry is unchanged, but pin it so the reflow never breaks
/// the upper sections).
#[test]
fn m7_settings_hit_desktop_path_and_watch_still_resolve() {
    let app = app_with_zones(vec![]);
    let source_count = app.desktop_sources.borrow().len();
    // The §2 path/watch boxes reserve space for the full source-card stack,
    // so at scroll 0 they sit below the visible body. Scroll the path input
    // to the body top exactly like the backup/plugins reachability tests do
    // (the §2 hit uses raw `scroll_offset_y`, no reserve fold), then sample
    // its centre. `settings_hit` reads `app.scroll_offset_y` directly.
    let flags = bentodesk_app::settings_panel::SettingsBodyFlags::new(
        app.crash_restart_enabled.get(),
        app.safe_start_after_hibernation.get(),
        false,
        false,
        bentodesk_app::settings_panel::UpdaterHeightKind::StatusOnly,
    );
    let body = bentodesk_app::settings_panel::settings_body_rect(app.viewport);
    let path_unscrolled = bentodesk_app::settings_panel::settings_desktop_path_input_rect(
        app.viewport,
        0.0,
        source_count,
    )
    .y;
    let content_h =
        bentodesk_app::settings_panel::settings_body_content_height(app.viewport, &flags);
    let max_scroll =
        bentodesk_app::settings_panel::settings_body_max_scroll(content_h, app.viewport);
    let scroll_off = (path_unscrolled - body.y).max(0.0).min(max_scroll);
    app.scroll_offset_y.set(scroll_off);

    let path_input = bentodesk_app::settings_panel::settings_desktop_path_input_rect(
        app.viewport,
        scroll_off,
        source_count,
    );
    assert!(
        path_input.y >= body.y && path_input.y < body.bottom(),
        "path input must scroll into the visible body (y={}, body=[{}, {}])",
        path_input.y,
        body.y,
        body.bottom(),
    );
    assert_eq!(
        settings_hit(
            &app,
            path_input.x + path_input.width * 0.5,
            path_input.y + path_input.height * 0.5,
        ),
        SettingsHit::EditDesktopPath,
    );
    let watch = bentodesk_app::settings_panel::settings_watch_textarea_rect(
        app.viewport,
        scroll_off,
        source_count,
    );
    // The watch textarea sits just below the path input; if it scrolled into
    // view, assert it resolves too (it may partially clip on a short body —
    // only assert when its centre is inside the body).
    let wy = watch.y + watch.height * 0.5;
    if wy >= body.y && wy < body.bottom() {
        assert_eq!(
            settings_hit(&app, watch.x + watch.width * 0.5, wy),
            SettingsHit::EditWatchValues,
        );
    }
}
