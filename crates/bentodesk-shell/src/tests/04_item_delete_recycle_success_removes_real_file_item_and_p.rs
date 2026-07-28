#[test]
fn item_delete_recycle_success_removes_real_file_item_and_persists() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-delete");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch dir");
    let source = state_dir.join("delete-me.txt");
    std::fs::write(&source, b"remove").expect("source file");
    let source_path = source.to_string_lossy().to_string();
    let zone_id = ZoneId(42);
    let item_id = {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut zone = Zone::new(zone_id, "Trash", 0, 0, 240, 160);
        let item_id = zone
            .add_item(Cow::Owned(source_path), Cow::Borrowed("hash"))
            .expect("item id");
        app.zones.add(zone);
        item_id
    };

    assert!(delete_item_file_to_recycle_bin_using(
        &root,
        zone_id,
        bentodesk_app::ItemId(item_id.0),
        |path| {
            assert_eq!(path, source.as_path());
            std::fs::remove_file(path).expect("simulated shell recycle");
            Ok(RecycleDeleteOutcome::Recycled)
        },
    ));

    {
        let app = root.app.borrow();
        assert!(app.dirty.get());
        assert!(app.zones.item(zone_id, item_id).is_none());
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Deleted file: delete-me.txt")
        );
        storage::write_zones_atomic(&zones_path, &app.zones).expect("persist zones");
        app.dirty.set(false);
    }
    assert!(!source.exists());
    let reloaded = storage::read_zones(&zones_path).expect("read persisted zones");
    assert!(reloaded.item(zone_id, item_id).is_none());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn item_delete_recycle_abort_keeps_file_item_and_reports_cancel() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-delete-abort");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch dir");
    let source = state_dir.join("keep-me.txt");
    std::fs::write(&source, b"keep").expect("source file");
    let source_path = source.to_string_lossy().to_string();
    let zone_id = ZoneId(43);
    let item_id = {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut zone = Zone::new(zone_id, "Trash", 0, 0, 240, 160);
        let item_id = zone
            .add_item(Cow::Owned(source_path), Cow::Borrowed("hash"))
            .expect("item id");
        app.zones.add(zone);
        item_id
    };

    assert!(delete_item_file_to_recycle_bin_using(
        &root,
        zone_id,
        bentodesk_app::ItemId(item_id.0),
        |path| {
            assert_eq!(path, source.as_path());
            Ok(RecycleDeleteOutcome::Aborted)
        },
    ));

    let app = root.app.borrow();
    assert!(!app.dirty.get(), "cancelled recycle must not mutate layout");
    assert!(app.zones.item(zone_id, item_id).is_some());
    assert!(source.exists());
    assert_eq!(
        app.item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Delete cancelled")
    );

    drop(app);
    let _ = std::fs::remove_dir_all(state_dir);
}

fn wide_pointer_has_double_nul(pointer: *const u16, max_units: usize) -> bool {
    assert!(!pointer.is_null());
    let mut previous = u16::MAX;
    for index in 0..max_units {
        // SAFETY: callers pass a pointer captured from the SHFILEOPSTRUCTW
        // built by `delete_path_to_recycle_bin_with`; the test bounds the
        // scan to a small fixed limit and only reads until a double NUL.
        let current = unsafe { *pointer.add(index) };
        if previous == 0 && current == 0 {
            return true;
        }
        previous = current;
    }
    false
}

#[test]
fn shell_file_list_from_path_is_double_nul_terminated() {
    let path = std::path::PathBuf::from(r"C:\Users\Alice\Desktop\note.txt");
    let encoded = shell_file_list_from_path(&path);
    assert_eq!(encoded.last(), Some(&0));
    assert_eq!(encoded.get(encoded.len().saturating_sub(2)), Some(&0));
    assert!(encoded.contains(&(b'n' as u16)));
}

#[test]
fn recycle_delete_uses_shell_delete_with_undo_flags() {
    let path = std::path::PathBuf::from(r"C:\Users\Alice\Desktop\note.txt");
    let mut captured_function = 0;
    let mut captured_flags = 0;
    let mut captured_to_is_null = false;
    let mut captured_has_double_nul = false;

    let outcome = delete_path_to_recycle_bin_with(&path, |operation| {
        captured_function = operation.wFunc;
        captured_flags = operation.fFlags;
        captured_to_is_null = operation.pTo.is_null();
        captured_has_double_nul = wide_pointer_has_double_nul(operation.pFrom, 128);
        0
    })
    .expect("shell delete should report success");

    assert_eq!(outcome, RecycleDeleteOutcome::Recycled);
    assert_eq!(captured_function, FO_DELETE);
    assert_eq!(captured_flags, recycle_delete_flags());
    assert_ne!(captured_flags & FOF_ALLOWUNDO as u16, 0);
    assert_ne!(captured_flags & FOF_WANTNUKEWARNING as u16, 0);
    assert!(captured_to_is_null);
    assert!(captured_has_double_nul);
}

#[test]
fn recycle_delete_reports_shell_abort_without_counting_success() {
    let path = std::path::PathBuf::from(r"C:\Users\Alice\Desktop\cancel.txt");

    let outcome = delete_path_to_recycle_bin_with(&path, |operation| {
        operation.fAnyOperationsAborted = 1;
        0
    })
    .expect("aborted shell delete still returns a structured outcome");

    assert_eq!(outcome, RecycleDeleteOutcome::Aborted);
}

#[test]
fn recycle_delete_surfaces_shell_error_code() {
    let path = std::path::PathBuf::from(r"C:\Users\Alice\Desktop\error.txt");

    let error = delete_path_to_recycle_bin_with(&path, |_operation| 123).expect_err("shell error");

    assert_eq!(error.to_string(), "SHFileOperationW failed: 123");
}

fn scratch_zones_path(label: &str) -> std::path::PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!("bentodesk-capsule-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root.join("zones.bin")
}

fn legacy_layout_with_zone(zone_id: &str, zone_name: &str, item_name: &str) -> LayoutData {
    let item_id = format!("{zone_id}01");
    let original_path = format!(r"C:\Users\Alice\Desktop\{item_name}");
    let hidden_path =
        format!(r"C:\Users\Alice\AppData\Roaming\BentoDesk\.bentodesk\{zone_id}\{item_name}");
    LayoutData {
        version: SmolStr::new_static("1.0.0"),
        zones: vec![BentoZone {
            id: SmolStr::new(zone_id),
            name: zone_name.to_string(),
            icon: SmolStr::new_static("folder"),
            position: RelativePosition {
                x_percent: 10.0,
                y_percent: 15.0,
            },
            expanded_size: RelativeSize {
                w_percent: 25.0,
                h_percent: 30.0,
            },
            items: vec![BentoItem {
                id: SmolStr::new(item_id),
                zone_id: SmolStr::new(zone_id),
                item_type: ItemType::File,
                name: item_name.to_string(),
                path: hidden_path.clone(),
                icon_hash: SmolStr::new_static("legacy-icon-hash"),
                grid_position: GridPosition {
                    col: 2,
                    row: 3,
                    col_span: 2,
                },
                is_wide: true,
                added_at: SmolStr::new_static("2026-05-17T00:00:00Z"),
                original_path: Some(original_path),
                hidden_path: Some(hidden_path),
                file_missing: false,
                icon_x: Some(120),
                icon_y: Some(240),
                tags: vec![SmolStr::new_static("legacy")],
            }],
            accent_color: Some(SmolStr::new_static("#22c55e")),
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            created_at: SmolStr::new_static("2026-05-17T00:00:00Z"),
            updated_at: SmolStr::new_static("2026-05-17T00:00:00Z"),
            capsule_size: SmolStr::new_static("large"),
            capsule_shape: SmolStr::new_static("rounded"),
            locked: true,
            visible: true,
            stack_id: None,
            stack_order: 0,
            alias: Some("Legacy Alias".to_string()),
            display_mode: Some(SmolStr::new_static("always")),
            live_folder_path: Some(r"C:\Users\Alice\Desktop\Live".to_string()),
        }],
        last_modified: SmolStr::new_static("2026-05-17T00:00:00Z"),
        coherence_id: Some(SmolStr::new_static("legacy-coherence")),
    }
}

fn sample_plugin_manifest_json(plugin_id: &str, name: &str) -> String {
    format!(
        r#"{{
  "id": "{plugin_id}",
  "name": "{name}",
  "version": "1.0.0",
  "type": "theme",
  "author": "Tester",
  "description": "Selected-stack archive plugin",
  "min_app_version": null,
  "icon": null
}}"#
    )
}

fn sample_plugin_theme_json(theme_id: &str, name: &str) -> String {
    format!(
        r##"{{
  "id": "{theme_id}",
  "name": "{name}",
  "is_builtin": false,
  "colors": {{
    "accent": "#22c55e",
    "background": "rgba(4, 30, 20, 0.8)",
    "text": "#ecfdf5",
    "border": "rgba(34, 197, 94, 0.2)"
  }},
  "capsule": {{
    "shape": "rounded",
    "size": "medium",
    "blur_radius": 16.0
  }},
  "animation": {{
    "expand_duration_ms": 180,
    "collapse_duration_ms": 160
  }},
  "glassmorphism": {{
    "blur": 16.0,
    "opacity": 0.8,
    "saturation": 1.3
  }}
}}"##
    )
}

fn write_plugin_archive(
    archive_path: &std::path::Path,
    plugin_id: &str,
    plugin_name: &str,
    theme_id: &str,
    theme_name: &str,
) {
    let file = std::fs::File::create(archive_path).expect("archive file");
    let mut writer = zip::ZipWriter::new(file);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("manifest.json", options)
        .expect("manifest entry");
    writer
        .write_all(sample_plugin_manifest_json(plugin_id, plugin_name).as_bytes())
        .expect("manifest bytes");
    writer
        .start_file("theme.json", options)
        .expect("theme entry");
    writer
        .write_all(sample_plugin_theme_json(theme_id, theme_name).as_bytes())
        .expect("theme bytes");
    writer.finish().expect("finish archive");
}

fn test_app_root() -> AppRoot {
    let (desktop_event_tx, desktop_events) = crossbeam_channel::unbounded();
    let (_, live_folder_events) = crossbeam_channel::unbounded();
    let (_, ghost_events) = crossbeam_channel::unbounded();
    let (power_event_tx, power_events) = crossbeam_channel::unbounded();
    let (updater_events_tx, updater_events) = crossbeam_channel::unbounded();
    let (_, rules_scheduler_events) = crossbeam_channel::unbounded();
    AppRoot {
        app: std::cell::RefCell::new(AppState::new()),
        registry: std::cell::RefCell::new(WindowRegistry::new()),
        dispatcher: EventDispatcher::new(),
        hovered: std::cell::RefCell::new(None),
        last_tick_ms: std::cell::Cell::new(0),
        frame_id: std::cell::Cell::new(0),
        recovery_state: std::cell::Cell::new(bentodesk_platform::RecoveryState::Healthy),
        last_recovery_at: std::cell::Cell::new(None),
        minibar_roster: std::cell::RefCell::new(MiniBarRoster::new()),
        minibars: std::cell::RefCell::new(smallvec::SmallVec::new()),
        zone_context_menu: std::cell::RefCell::new(None),
        item_context_menu: std::cell::RefCell::new(None),
        pending_item_drag_out: std::cell::RefCell::new(None),
        pending_stack_drop_bloom: std::cell::Cell::new(None),
        item_drag_out_active: std::cell::Cell::new(false),
        tray_context_menu: std::cell::RefCell::new(None),
        tray_context_menu_consumed: std::cell::Cell::new(false),
        hotkey_bindings: std::cell::RefCell::new(default_hotkey_bindings()),
        global_hotkeys: std::cell::RefCell::new(smallvec::SmallVec::new()),
        tray_registered: std::cell::Cell::new(false),
        tray_retry_attempts: std::cell::Cell::new(0),
        tray_uid_only: std::cell::Cell::new(false),
        desktop_watcher: std::cell::RefCell::new(None),
        desktop_event_tx,
        desktop_events,
        live_folder_events,
        live_folder_rehydrated: std::cell::Cell::new(true),
        ghost_events,
        power_event_tx,
        power_events,
        updater: bentodesk_backend::updater::Updater::new(updater_events_tx),
        updater_events,
        rules_scheduler_events,
        timeline_buffer: std::cell::RefCell::new(
            bentodesk_backend::timeline::TimelineBuffer::default(),
        ),
    }
}

fn seed_test_zone(root: &AppRoot, id: u64, title: &str) {
    root.app
        .borrow_mut()
        .zones
        .add(Zone::new(ZoneId(id), title.to_string(), 24, 32, 180, 120));
}
#[test]
fn tooltip_payload_helpers_seed_and_clear_render_state() {
    let root = test_app_root();

    assert!(show_tooltip_payload(
        &root,
        &smol_str::SmolStr::new_static("Open settings")
    ));
    {
        let app = root.app.borrow();
        let active = app.active_tooltip.borrow();
        assert_eq!(
            active.as_ref().map(|session| session.text.as_str()),
            Some("Open settings")
        );
    }

    assert!(!show_tooltip_payload(
        &root,
        &smol_str::SmolStr::new_static("Open settings")
    ));
    assert!(show_tooltip_payload(
        &root,
        &smol_str::SmolStr::new_static("Open search")
    ));
    {
        let app = root.app.borrow();
        let active = app.active_tooltip.borrow();
        assert_eq!(
            active.as_ref().map(|session| session.text.as_str()),
            Some("Open search")
        );
    }

    assert!(hide_tooltip_payload(&root));
    assert!(root.app.borrow().active_tooltip.borrow().is_none());
    assert!(!hide_tooltip_payload(&root));
}

#[test]
fn settings_close_surface_hides_active_tooltip() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        app.set_settings_close_hover(true);
    }

    assert!(show_tooltip_payload(
        &root,
        &smol_str::SmolStr::new_static("Save settings")
    ));

    let _hidden = close_settings_surface(&root);

    let app = root.app.borrow();
    assert!(!app.settings_open.get());
    assert!(!app.settings_close_hover.get());
    assert!(app.active_tooltip.borrow().is_none());
}

#[test]
fn settings_outside_click_uses_visible_panel_not_full_workarea_host() {
    let panel = super::settings_panel_client_device_rect(
        super::RECT {
            left: 0,
            top: 0,
            right: 2560,
            bottom: 1368,
        },
        144,
    );
    assert_eq!(
        (panel.left, panel.top, panel.right, panel.bottom),
        (920, 137, 1640, 1231)
    );

    assert!(!super::settings_outside_click_should_close(
        true, false, true, true
    ));
    assert!(super::settings_outside_click_should_close(
        true, false, true, false
    ));
    assert!(super::settings_outside_click_should_close(
        false, false, false, true
    ));
    assert!(!super::settings_outside_click_should_close(
        false, false, true, false
    ));
    assert!(super::settings_outside_click_should_close(
        false, true, true, false
    ));

    // An owned common-dialog click is suppressed while held and for the
    // first released poll; only the following independent click may close
    // Settings.
    assert_eq!(
        super::settings_owned_dialog_guard_transition(true, true),
        (true, true)
    );
    assert_eq!(
        super::settings_owned_dialog_guard_transition(true, false),
        (true, false)
    );
    assert_eq!(
        super::settings_owned_dialog_guard_transition(false, false),
        (false, false)
    );
}

#[test]
fn settings_controls_do_not_spawn_native_tooltip_windows() {
    let root = test_app_root();
    let app = root.app.borrow();

    assert_eq!(
        super::settings_tooltip_text_for_hit(
            &app,
            super::ui::SettingsHit::ToggleStartupHighPriority,
        ),
        None,
    );
    assert_eq!(
        super::settings_tooltip_text_for_hit(&app, super::ui::SettingsHit::SaveSettings),
        None,
    );
}

#[test]
fn tooltip_hover_item_producer_queues_show_and_hide() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(88), "Docs", 0, 0, 240, 160);
        let item_id = zone
            .add_item_with_metadata(
                Cow::Owned("C:/Users/BentoDeskTest/Desktop/.bentodesk/docs/contract.pdf".to_owned()),
                Some("C:/Users/BentoDeskTest/Desktop/contract.pdf"),
                Cow::Borrowed("hash"),
                Some(Cow::Owned("C:/Users/BentoDeskTest/Desktop/contract.pdf".to_owned())),
                Some(Cow::Owned(
                    "C:/Users/BentoDeskTest/Desktop/.bentodesk/docs/contract.pdf".to_owned(),
                )),
            )
            .expect("item id allocated");
        assert_eq!(item_id, ZoneItemId(1));
        app.zones.add(zone);
        // Wave C — item-grid hover is only meaningful when the zone is
        // in its expanded form. Pill mode hides items completely.
        app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    }

    let show = {
        let app = root.app.borrow();
        // M2① grid row 0 starts below the 48-DIP header (zone_top 0 + 48);
        // hover at y=70 lands on the first card, clear of the header band.
        tooltip_command_for_hover(&app, bentodesk_app::WindowHandle::NULL, 24.0, 70.0)
    };
    match show {
        Some(Command::ShowTooltip { anchor, text }) => {
            assert_eq!(anchor, bentodesk_app::WindowHandle::NULL);
            assert!(
                text.as_str().contains("contract.pdf")
                    && text.as_str().contains("C:/Users/BentoDeskTest/Desktop/contract.pdf")
            );
        }
        other => panic!("expected ShowTooltip for hovered item, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        assert!(app.show_tooltip_text(SmolStr::new_static("contract.pdf")));
    }
    let hide = {
        let app = root.app.borrow();
        tooltip_command_for_hover(&app, bentodesk_app::WindowHandle::NULL, 900.0, 900.0)
    };
    assert!(matches!(hide, Some(Command::HideTooltip)));
}

#[test]
fn main_tooltip_suppresses_stack_bloom_hover_anchor() {
    let root = test_app_root();
    let win = WindowState::new();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let mut anchor = Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130);
        anchor.add_item(r"C:\Users\BentoDeskTest\Desktop\contract.pdf", "document");
        app.zones.add(anchor);
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
    }

    let app = root.app.borrow();
    let hover_x = 124.0;
    let hover_y = 118.0;
    assert!(stack_bloom_hover_suppresses_main_tooltip(
        &app, hover_x, hover_y
    ));
    assert!(
        tooltip_command_for_main_hover(
            &app,
            &win,
            bentodesk_app::WindowHandle::NULL,
            hover_x,
            hover_y
        )
        .is_none()
    );
}

#[test]
fn tooltip_never_spawns_a_second_auxiliary_surface() {
    let main = bentodesk_app::WindowHandle(41);
    let auxiliary = bentodesk_app::WindowHandle(42);

    assert!(!tooltip_uses_aux_surface(main, Some(main), false));
    assert!(!tooltip_uses_aux_surface(auxiliary, Some(main), true));
    assert!(!tooltip_uses_aux_surface(auxiliary, Some(main), false));
    assert!(!tooltip_uses_aux_surface(
        bentodesk_app::WindowHandle::NULL,
        None,
        false
    ));
}

fn rect_center(rect: bentodesk_style::Rect) -> (f32, f32) {
    (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
}

fn tooltip_checkpoint_meta(
    id: &'static str,
    summary: &'static str,
) -> bentodesk_backend::timeline::CheckpointMeta {
    bentodesk_backend::timeline::CheckpointMeta {
        id: SmolStr::new_static(id),
        captured_at: SmolStr::new_static("2026-05-12T00:00:00Z"),
        trigger: SmolStr::new_static("manual"),
        delta_summary: summary.to_owned(),
        pinned: false,
        zone_count: 2,
        item_count: 3,
    }
}
