#[test]
fn tooltip_hover_timeline_row_text_tracks_cursor() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        app.timeline_panel.borrow_mut().set_entries(vec![
            tooltip_checkpoint_meta("cp-1", "first save"),
            tooltip_checkpoint_meta("cp-2", "second save"),
        ]);
    }
    let (row_x, row_y) = {
        let app = root.app.borrow();
        rect_center(bentodesk_app::business::timeline::panel::timeline_row_rect(app.viewport, 1))
    };

    let unselected = {
        let app = root.app.borrow();
        tooltip_command_for_timeline_hover(&app, bentodesk_app::WindowHandle::NULL, row_x, row_y)
    };
    match unselected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Select checkpoint second save");
        }
        other => panic!("expected unselected Timeline row tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        assert!(app.timeline_panel.borrow_mut().select_index(1));
    }
    let selected = {
        let app = root.app.borrow();
        tooltip_command_for_timeline_hover(&app, bentodesk_app::WindowHandle::NULL, row_x, row_y)
    };
    match selected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Selected checkpoint second save");
        }
        other => panic!("expected selected Timeline row tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_snapshot_picker_button_producer_queues_show() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 640.0,
            height: 520.0,
        };
    }
    let load = bentodesk_app::business::timeline::snapshot_picker::SNAPSHOT_PICKER_ACTION_BUTTONS
            .iter()
            .copied()
            .find(|spec| {
                spec.hit
                    == bentodesk_app::business::timeline::snapshot_picker::SnapshotPickerPointerHit::Load
            })
            .expect("snapshot load button");
    let (load_x, load_y) = {
        let app = root.app.borrow();
        rect_center(
            bentodesk_app::business::timeline::snapshot_picker::snapshot_picker_button_rect(
                app.viewport,
                load,
            ),
        )
    };

    let show = {
        let app = root.app.borrow();
        tooltip_command_for_snapshot_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            load_x,
            load_y,
        )
    };
    match show {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Load selected snapshot");
        }
        other => panic!("expected SnapshotPicker load tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_snapshot_picker_row_text_tracks_cursor() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 640.0,
            height: 520.0,
        };
        app.snapshot_picker.borrow_mut().set_entries(vec![
            tooltip_snapshot("snap-1", "First snapshot"),
            tooltip_snapshot("snap-2", "Second snapshot"),
        ]);
    }
    let (row_x, row_y) = {
        let app = root.app.borrow();
        rect_center(
            bentodesk_app::business::timeline::snapshot_picker::snapshot_picker_row_rect(
                app.viewport,
                1,
            ),
        )
    };

    let unselected = {
        let app = root.app.borrow();
        tooltip_command_for_snapshot_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            row_x,
            row_y,
        )
    };
    match unselected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Select snapshot Second snapshot");
        }
        other => panic!("expected unselected SnapshotPicker row tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        assert!(app.snapshot_picker.borrow_mut().select_index(1));
    }
    let selected = {
        let app = root.app.borrow();
        tooltip_command_for_snapshot_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            row_x,
            row_y,
        )
    };
    match selected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Selected snapshot Second snapshot");
        }
        other => panic!("expected selected SnapshotPicker row tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_icon_picker_text_tracks_selected_slot_and_hide() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.icon_picker.borrow_mut().replace(IconPickerSession {
            zone_id: Some(ZoneId(7)),
            selected_icon: SmolStr::new_static("folder"),
        });
    }
    let (slot0_x, slot0_y, slot1_x, slot1_y) = {
        let app = root.app.borrow();
        let slot0 = icon_picker_slot_rect(app.viewport, 0);
        let slot1 = icon_picker_slot_rect(app.viewport, 1);
        (
            slot0.x + slot0.width * 0.5,
            slot0.y + slot0.height * 0.5,
            slot1.x + slot1.width * 0.5,
            slot1.y + slot1.height * 0.5,
        )
    };

    let selected = {
        let app = root.app.borrow();
        tooltip_command_for_icon_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            slot0_x,
            slot0_y,
        )
    };
    match selected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Selected icon folder");
        }
        other => panic!("expected selected IconPicker tooltip, got {other:?}"),
    }

    let unselected = {
        let app = root.app.borrow();
        tooltip_command_for_icon_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            slot1_x,
            slot1_y,
        )
    };
    match unselected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Choose icon document");
        }
        other => panic!("expected unselected IconPicker tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        assert!(app.show_tooltip_text(SmolStr::new_static("Choose icon document")));
    }
    let hide = {
        let app = root.app.borrow();
        tooltip_command_for_icon_picker_hover(&app, bentodesk_app::WindowHandle::NULL, 0.0, 0.0)
    };
    assert!(matches!(hide, Some(Command::HideTooltip)));
}

#[test]
fn tooltip_hover_palette_picker_text_tracks_swatch_and_clear_state() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 320.0,
            height: 240.0,
        };
        app.palette_picker
            .borrow_mut()
            .replace(PalettePickerSession {
                target: PaletteTarget::ThemeBase,
                selected_accent: Some(SmolStr::new_static("#3b82f6")),
            });
    }
    let (blue_x, blue_y, clear_x, clear_y) = {
        let app = root.app.borrow();
        let blue = palette_picker_swatch_rect(app.viewport, 1);
        let clear = palette_picker_clear_rect(app.viewport);
        (
            blue.x + blue.width * 0.5,
            blue.y + blue.height * 0.5,
            clear.x + clear.width * 0.5,
            clear.y + clear.height * 0.5,
        )
    };

    let selected = {
        let app = root.app.borrow();
        tooltip_command_for_palette_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            blue_x,
            blue_y,
        )
    };
    match selected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Selected color blue #3b82f6");
        }
        other => panic!("expected selected PalettePicker swatch tooltip, got {other:?}"),
    }

    let clear = {
        let app = root.app.borrow();
        tooltip_command_for_palette_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            clear_x,
            clear_y,
        )
    };
    match clear {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Clear accent color");
        }
        other => panic!("expected PalettePicker clear tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        app.palette_picker
            .borrow_mut()
            .as_mut()
            .expect("palette session")
            .selected_accent = None;
    }
    let already_clear = {
        let app = root.app.borrow();
        tooltip_command_for_palette_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            clear_x,
            clear_y,
        )
    };
    match already_clear {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Accent color already clear");
        }
        other => panic!("expected PalettePicker already-clear tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_suggestor_omits_row_bubble_but_keeps_action_help() {
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

    let (row_x, row_y, apply_x, apply_y) = {
        let app = root.app.borrow();
        let row = smart_group_suggestor::suggestor_row_rect(app.viewport, 0);
        let apply = smart_group_suggestor::suggestor_apply_rect(app.viewport, 0);
        (row.x + 12.0, row.y + 12.0, apply.x + 2.0, apply.y + 2.0)
    };

    let row_show = {
        let app = root.app.borrow();
        tooltip_command_for_suggestor_hover(&app, bentodesk_app::WindowHandle::NULL, row_x, row_y)
    };
    assert!(
        row_show.is_none(),
        "the fully labelled suggestion card must not spawn a redundant mini tooltip"
    );

    let apply_show = {
        let app = root.app.borrow();
        tooltip_command_for_suggestor_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            apply_x,
            apply_y,
        )
    };
    match apply_show {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Apply suggestion Documents (4 files)");
        }
        other => panic!("expected Suggestor apply tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        assert!(app.show_tooltip_text(SmolStr::new_static("Apply suggestion Documents (4 files)")));
    }
    let hide = {
        let app = root.app.borrow();
        tooltip_command_for_suggestor_hover(&app, bentodesk_app::WindowHandle::NULL, 0.0, 0.0)
    };
    assert!(matches!(hide, Some(Command::HideTooltip)));
}

#[test]
fn tooltip_hover_suggestor_preview_file_text_tracks_selection_state() {
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
    let (preview_x, preview_y) = {
        let app = root.app.borrow();
        let preview = smart_group_suggestor::suggestor_preview_file_rect(app.viewport, 0);
        (preview.x + 2.0, preview.y + 2.0)
    };

    let selected = {
        let app = root.app.borrow();
        tooltip_command_for_suggestor_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            preview_x,
            preview_y,
        )
    };
    match selected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Deselect preview file doc0.pdf");
        }
        other => panic!("expected selected preview-file tooltip, got {other:?}"),
    }

    let _ = handle_suggestor_keydown(&root, VK_SPACE_KEY, std::ptr::null_mut());
    let deselected = {
        let app = root.app.borrow();
        tooltip_command_for_suggestor_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            preview_x,
            preview_y,
        )
    };
    match deselected {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Select preview file doc0.pdf");
        }
        other => panic!("expected deselected preview-file tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_minibar_text_tracks_body_and_unpin_targets() {
    let root = test_app_root();
    let viewport = bentodesk_style::Size {
        width: 280.0,
        height: 80.0,
    };
    {
        let app = root.app.borrow();
        app.upsert_minibar(
            ZoneId(42),
            MiniBar::new(ui::HIDE_PATH, SmolStr::new_static("Docs"), 43),
        );
    }

    let body = {
        let app = root.app.borrow();
        tooltip_command_for_minibar_hover(
            &app,
            viewport,
            bentodesk_app::WindowHandle::NULL,
            50.0,
            40.0,
        )
    };
    match body {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Pinned zone Docs");
        }
        other => panic!("expected MiniBar body tooltip, got {other:?}"),
    }

    let unpin = {
        let app = root.app.borrow();
        tooltip_command_for_minibar_hover(
            &app,
            viewport,
            bentodesk_app::WindowHandle::NULL,
            250.0,
            40.0,
        )
    };
    match unpin {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Unpin minibar Docs");
        }
        other => panic!("expected MiniBar unpin tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        assert!(app.show_tooltip_text(SmolStr::new_static("Unpin minibar Docs")));
    }
    let hide = {
        let app = root.app.borrow();
        tooltip_command_for_minibar_hover(
            &app,
            viewport,
            bentodesk_app::WindowHandle::NULL,
            320.0,
            40.0,
        )
    };
    assert!(matches!(hide, Some(Command::HideTooltip)));
}

#[test]
fn tooltip_hover_capsule_picker_text_tracks_rows_hint_error_and_empty_state() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 480.0,
            height: 600.0,
        };
        let mut entries = smallvec::SmallVec::new();
        entries.push(CapsuleEntry::new(
            "cap-1",
            "Writing mode",
            "briefcase",
            "2026-05-12T10:00:00Z",
        ));
        app.capsule_picker.borrow_mut().set_entries(entries);
    }

    let row = capsule_picker_row_rect(
        Size {
            width: 480.0,
            height: 600.0,
        },
        0,
    );
    let show_row = {
        let app = root.app.borrow();
        tooltip_command_for_capsule_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            row.x + 8.0,
            row.y + 8.0,
        )
    };
    match show_row {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(
                text.as_str(),
                "Selected capsule Writing mode captured 2026-05-12T10:00:00Z"
            );
        }
        other => panic!("expected CapsulePicker row tooltip, got {other:?}"),
    }

    let hint = capsule_picker_hint_rect(Size {
        width: 480.0,
        height: 600.0,
    });
    let show_hint = {
        let app = root.app.borrow();
        tooltip_command_for_capsule_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            hint.x + 8.0,
            hint.y + 8.0,
        )
    };
    match show_hint {
        Some(Command::ShowTooltip { text, .. }) => {
            assert!(text.as_str().contains("C capture"));
        }
        other => panic!("expected CapsulePicker hint tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        app.capsule_picker
            .borrow_mut()
            .set_error(Some(SmolStr::new_static("restore failed")));
    }
    let error = capsule_picker_error_rect(Size {
        width: 480.0,
        height: 600.0,
    });
    let show_error = {
        let app = root.app.borrow();
        tooltip_command_for_capsule_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            error.x + 8.0,
            error.y + 8.0,
        )
    };
    match show_error {
        Some(Command::ShowTooltip { text, .. }) => {
            assert_eq!(text.as_str(), "Capsule error: restore failed");
        }
        other => panic!("expected CapsulePicker error tooltip, got {other:?}"),
    }

    {
        let app = root.app.borrow();
        let mut picker = app.capsule_picker.borrow_mut();
        picker.set_entries(smallvec::SmallVec::new());
        picker.set_error(None);
    }
    let empty = capsule_picker_empty_rect(Size {
        width: 480.0,
        height: 600.0,
    });
    let show_empty = {
        let app = root.app.borrow();
        tooltip_command_for_capsule_picker_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            empty.x + 8.0,
            empty.y + 8.0,
        )
    };
    match show_empty {
        Some(Command::ShowTooltip { text, .. }) => {
            assert!(text.as_str().contains("No capsules yet"));
        }
        other => panic!("expected CapsulePicker empty tooltip, got {other:?}"),
    }
}

#[test]
fn tooltip_hover_zone_editor_never_opens_a_detached_explanation_strip() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 420.0,
            height: 380.0,
        };
        app.zone_editor.borrow_mut().replace(ZoneEditorSession {
            zone_id: ZoneId(5),
            draft_name: "Docs".to_owned(),
            draft_icon: SmolStr::new_static("folder"),
            draft_accent_color: Some(SmolStr::new_static("#22C55E")),
            draft_grid_columns: 5,
            draft_capsule_size: SmolStr::new_static("large"),
            draft_capsule_shape: SmolStr::new_static("rounded"),
        });
    }

    let viewport = Size {
        width: 420.0,
        height: 380.0,
    };
    let save = zone_editor_save_rect(viewport);
    let no_tooltip = {
        let app = root.app.borrow();
        tooltip_command_for_zone_editor_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            save.x + 8.0,
            save.y + 8.0,
        )
    };
    assert!(no_tooltip.is_none());

    assert!(
        root.app
            .borrow()
            .show_tooltip_text(SmolStr::new_static("stale tooltip"))
    );
    let hide_stale = {
        let app = root.app.borrow();
        tooltip_command_for_zone_editor_hover(
            &app,
            bentodesk_app::WindowHandle::NULL,
            save.x + 8.0,
            save.y + 8.0,
        )
    };
    assert!(matches!(hide_stale, Some(Command::HideTooltip)));
}
