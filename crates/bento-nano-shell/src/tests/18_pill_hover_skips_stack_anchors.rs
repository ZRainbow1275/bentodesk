#[test]
fn pill_hover_skips_stack_anchors() {
    // #4 (2026-06-02) — stack anchors don't run the pill→panel morph
    // (stack_bloom owns the hover affordance); pill morph must not steal
    // that animation slot. A collapsed anchor still renders as the compact
    // pill, but via the non-morph pill paint path.
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
    }
    {
        let app = root.app.borrow();
        assert!(!update_zone_pill_hover(&app, Some(ZoneId(1)), 1_000));
        assert_eq!(app.zone_pill_anim_zone.get(), None);
    }
}

#[test]
fn stack_bloom_petal_click_opens_floating_preview_without_management_tray() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(update_stack_bloom_hover(&app, Some(ZoneId(1)), 1_000));
        app.stack_bloom_progress.set(1.0);
    }
    let petal = {
        let app = root.app.borrow();
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        stack_tray::stack_bloom_petal_rects(app.viewport, anchor, 2)[1]
    };

    assert!(handle_stack_bloom_lbutton_up(
        &root,
        std::ptr::null_mut(),
        petal.x + 4.0,
        petal.y + 4.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::ToggleStackBloomPreview(ZoneId(1), ZoneId(2)))
    ));
}

#[test]
fn stack_capsule_outranks_petals_during_bloom_entry_overlap() {
    let root = test_app_root();
    let (x, y) = {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(update_stack_bloom_hover(&app, Some(ZoneId(1)), 1_000));
        app.stack_bloom_progress.set(0.0);

        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        let capsule = zone_pill_geometry::stack_capsule_layout_for_zone(anchor, 2).rect;
        let x = capsule.x + capsule.width * 0.5;
        let y = capsule.y + capsule.height * 0.5;
        assert_eq!(
            stack_tray::stack_bloom_hit_test_at(app.viewport, anchor, 2, 0.0, x, y),
            Some(0),
            "fixture must cover the transient animated-petal/capsule overlap"
        );
        (x, y)
    };

    assert_eq!(
        stack_bloom_hit_for_point(&root.app.borrow(), x, y),
        None,
        "capsule hover/click must not arm or commit an animated petal preview"
    );
}

fn stack_bloom_click_fixture() -> (AppRoot, f32, f32) {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(update_stack_bloom_hover(&app, Some(ZoneId(1)), 1_000));
        app.stack_bloom_progress.set(1.0);
    }
    let petal = {
        let app = root.app.borrow();
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        stack_tray::stack_bloom_petal_rects(app.viewport, anchor, 2)[1]
    };
    (root, petal.x + 4.0, petal.y + 4.0)
}

fn open_stack_bloom_preview() -> AppRoot {
    let (root, x, y) = stack_bloom_click_fixture();
    assert!(handle_stack_bloom_lbutton_up(
        &root,
        std::ptr::null_mut(),
        x,
        y
    ));
    consume_dispatcher(&root, std::ptr::null_mut());
    root
}

fn stack_bloom_preview_rect_for_test(root: &AppRoot) -> bento_nano_style::Rect {
    let app = root.app.borrow();
    let state = app.stack_tray.borrow().clone().expect("preview state");
    let anchor = app.zones.get(state.anchor_zone_id).expect("anchor");
    let members = app.zones.stack_member_ids(anchor.id).expect("members");
    let index = members
        .iter()
        .position(|member| *member == state.selected_member_id)
        .expect("selected member");
    let member = app
        .zones
        .get(state.selected_member_id)
        .expect("selected zone");
    let petals = stack_tray::stack_bloom_petal_rects(app.viewport, anchor, members.len());
    let petal = petals[index];
    stack_tray::focused_bloom_preview_rect(app.viewport, petal, &petals, member)
}

#[test]
fn stack_bloom_pointer_dispatch_uses_explicit_visible_bloom_state() {
    let (root, x, y) = stack_bloom_click_fixture();

    assert!(handle_stack_bloom_lbutton_up(
        &root,
        std::ptr::null_mut(),
        x,
        y
    ));
    super::consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    let tray = app.stack_tray.borrow();
    let tray = tray
        .as_ref()
        .expect("pointer dispatch must open the floating preview");
    assert!(tray.is_bloom_preview());
    assert_eq!(tray.anchor_zone_id, ZoneId(1));
    assert_eq!(tray.selected_member_id, ZoneId(2));
    assert_eq!(app.stack_bloom_anchor.get(), Some(ZoneId(1)));
    assert!(!app.stack_bloom_leaving.get());
}

#[test]
fn stack_bloom_preview_click_toggles_and_switches_members_without_management_mode() {
    let (root, x, y) = stack_bloom_click_fixture();
    assert!(handle_stack_bloom_lbutton_up(
        &root,
        std::ptr::null_mut(),
        x,
        y
    ));
    consume_dispatcher(&root, std::ptr::null_mut());

    root.dispatcher
        .push(Command::PreviewStackMember(ZoneId(1), ZoneId(1)));
    consume_dispatcher(&root, std::ptr::null_mut());
    {
        let app = root.app.borrow();
        let state = app.stack_tray.borrow();
        let state = state.as_ref().expect("switch keeps the preview open");
        assert!(state.is_bloom_preview());
        assert_eq!(state.selected_member_id, ZoneId(1));
    }

    root.dispatcher
        .push(Command::PreviewStackMember(ZoneId(1), ZoneId(1)));
    consume_dispatcher(&root, std::ptr::null_mut());
    assert!(root.app.borrow().stack_tray.borrow().is_none());
}

#[test]
fn stack_bloom_preview_body_keeps_bloom_open_and_consumes_pointer() {
    let root = open_stack_bloom_preview();
    let preview = stack_bloom_preview_rect_for_test(&root);
    let body_x = preview.x + 16.0;
    let body_y = preview.y + preview.height - 16.0;
    {
        let app = root.app.borrow();
        assert_eq!(
            stack_bloom_preview_hit_for_point(&app, body_x, body_y)
                .map(|(anchor, member, _)| (anchor, member)),
            Some((ZoneId(1), ZoneId(2)))
        );
        assert_eq!(
            stack_bloom_hover_anchor_for_point(&app, body_x, body_y),
            Some(ZoneId(1))
        );
        assert!(!update_stack_bloom_hover(&app, Some(ZoneId(1)), 2_000));
        assert!(!app.stack_bloom_leaving.get());
        assert!(handle_stack_bloom_preview_lbutton_down(
            &app,
            std::ptr::null_mut(),
            body_x,
            body_y
        ));
    }
    assert!(handle_stack_bloom_preview_lbutton_up(
        &root,
        std::ptr::null_mut(),
        body_x,
        body_y
    ));
    assert!(root.app.borrow().stack_tray.borrow().is_some());
}

#[test]
fn stack_bloom_preview_header_actions_close_or_open_search() {
    let root = open_stack_bloom_preview();
    let close =
        stack_tray::focused_bloom_preview_close_rect(stack_bloom_preview_rect_for_test(&root));
    assert!(handle_stack_bloom_preview_lbutton_up(
        &root,
        std::ptr::null_mut(),
        close.x + close.width * 0.5,
        close.y + close.height * 0.5
    ));
    assert!(root.app.borrow().stack_tray.borrow().is_none());
    assert_eq!(root.app.borrow().stack_bloom_anchor.get(), Some(ZoneId(1)));

    let root = open_stack_bloom_preview();
    let search =
        stack_tray::focused_bloom_preview_search_rect(stack_bloom_preview_rect_for_test(&root));
    assert!(handle_stack_bloom_preview_lbutton_up(
        &root,
        std::ptr::null_mut(),
        search.x + search.width * 0.5,
        search.y + search.height * 0.5
    ));
    {
        let app = root.app.borrow();
        let preview = app.stack_tray.borrow();
        assert!(
            preview
                .as_ref()
                .is_some_and(|state| state.is_bloom_preview())
        );
        assert_eq!(app.zone_search_target.get(), Some(ZoneId(2)));
    }
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
}

#[test]
fn stack_bloom_preview_item_single_click_arms_drag_and_double_click_opens() {
    let root = open_stack_bloom_preview();
    {
        let mut app = root.app.borrow_mut();
        let member = app.zones.get_mut(ZoneId(2)).expect("member");
        member.w = 320;
        member.h = 360;
        member.items.push(ZoneItem::new(
            ZoneItemId(41),
            r"C:\Users\Alice\Desktop\report.txt",
            "text-icon",
            0,
            0,
        ));
    }
    let preview = stack_bloom_preview_rect_for_test(&root);
    let card = {
        let app = root.app.borrow();
        let member = app.zones.get(ZoneId(2)).expect("member");
        highlight_overlay::item_card_rect_for_flow_slot_in_panel(member, preview, 0, false, 0.0).0
    };
    let x = card.x + card.width * 0.5;
    let y = card.y + card.height * 0.5;

    {
        let app = root.app.borrow();
        assert_eq!(
            stack_bloom_preview_item_hit_for_point(&app, x, y),
            Some((ZoneId(1), ZoneId(2), ZoneItemId(41)))
        );
        assert_eq!(item_drag_target_zone_for_point(&app, x, y), Some(ZoneId(2)));
        assert_eq!(
            item_grid_position_for_drag_point(&app, ZoneId(2), x, y),
            Some((0, 0))
        );
        assert!(handle_stack_bloom_preview_lbutton_down(
            &app,
            std::ptr::null_mut(),
            x,
            y,
        ));
        assert!(app.item_drag.borrow().is_some());
    }
    assert!(
        !handle_stack_bloom_preview_lbutton_up(&root, std::ptr::null_mut(), x, y),
        "preview must let the shared item-drag release path consume mouse-up"
    );

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
    let command = {
        let app = root.app.borrow();
        item_open_command_for_double_click(&app, x, y)
    };
    assert!(matches!(
        command,
        Some(Command::OpenItemFile(ZoneId(2), bento_nano_app::ItemId(41)))
    ));
    assert!(root.app.borrow().stack_tray.borrow().is_some());
}

#[test]
fn stack_drop_arms_bloom_without_invisible_settled_petal_hits() {
    let root = test_app_root();
    let petal = {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        reveal_stack_at_drop_pointer(&app, ZoneId(1), 1_000);
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        stack_tray::stack_bloom_petal_rects(app.viewport, anchor, 2)[1]
    };

    let app = root.app.borrow();
    assert_eq!(app.hovered_zone.get(), Some(ZoneId(1)));
    assert_eq!(app.stack_bloom_anchor.get(), Some(ZoneId(1)));
    assert_eq!(app.stack_bloom_progress.get(), 0.0);
    assert!(app.stack_tray.borrow().is_none());
    assert_eq!(
        stack_bloom_hit_for_point(&app, petal.x + 4.0, petal.y + 4.0),
        None
    );
}

#[test]
fn pointer_stack_drop_blooms_on_the_same_dispatch_turn() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        app.selected_zone.set(Some(ZoneId(1)));
    }

    root.pending_stack_drop_bloom.set(Some(ZoneId(1)));
    root.dispatcher
        .push(Command::StackZone(ZoneId(1), ZoneId(2)));
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    assert_eq!(root.pending_stack_drop_bloom.get(), None);
    assert_eq!(app.zones.stack_anchor_for(ZoneId(2)), Some(ZoneId(1)));
    assert_eq!(app.selected_zone.get(), None);
    assert_eq!(app.hovered_zone.get(), Some(ZoneId(1)));
    assert_eq!(app.stack_bloom_anchor.get(), Some(ZoneId(1)));
    assert!(!app.stack_bloom_leaving.get());
    assert_eq!(app.stack_bloom_progress.get(), 0.0);
    assert!(app.stack_tray.borrow().is_none());
}

#[test]
fn stack_capsule_click_toggles_bloom_without_expanding_panel() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        app.selected_zone.set(None);
        app.hovered_zone.set(Some(ZoneId(1)));
        clear_stack_bloom_surface(&app);

        assert!(toggle_stack_bloom_from_capsule_click(
            &app,
            ZoneId(1),
            2_000
        ));
        assert_eq!(app.selected_zone.get(), None);
        assert_eq!(app.stack_bloom_anchor.get(), Some(ZoneId(1)));
        assert!(!app.zone_pill_body_visible(app.zones.get(ZoneId(1)).unwrap()));

        assert!(toggle_stack_bloom_from_capsule_click(
            &app,
            ZoneId(1),
            2_500
        ));
        assert!(app.stack_bloom_leaving.get());
        assert_eq!(app.selected_zone.get(), None);
    }
}

#[test]
fn stack_capsule_click_and_always_modes_ignore_pointer_enter() {
    let root = test_app_root();
    let mut app = root.app.borrow_mut();
    app.zones
        .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
    app.zones
        .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
    assert!(app.zones.stack(ZoneId(1), ZoneId(2)));

    app.set_zone_display_mode(ZoneDisplayMode::Click);
    app.hovered_zone.set(None);
    on_hover_target_changed(&app, Some(ZoneId(1)), 900);
    assert_eq!(app.stack_bloom_anchor.get(), None);
    assert!(app.stack_tray.borrow().is_none());
    assert!(toggle_stack_bloom_from_capsule_click(
        &app,
        ZoneId(1),
        1_000
    ));
    assert!(
        app.stack_tray
            .borrow()
            .as_ref()
            .is_some_and(stack_tray::StackTrayState::is_management)
    );
    assert_eq!(app.stack_bloom_anchor.get(), None);
    assert!(toggle_stack_bloom_from_capsule_click(
        &app,
        ZoneId(1),
        1_100
    ));
    assert!(app.stack_tray.borrow().is_none());

    app.set_zone_display_mode(ZoneDisplayMode::Always);
    app.hovered_zone.set(None);
    on_hover_target_changed(&app, Some(ZoneId(1)), 2_000);
    assert!(app.stack_tray.borrow().is_none());
    assert_eq!(app.stack_bloom_anchor.get(), None);
    assert!(!toggle_stack_bloom_from_capsule_click(
        &app,
        ZoneId(1),
        2_100
    ));
}

fn stack_tray_row_fixture() -> (AppRoot, f32, f32) {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        app.stack_tray
            .borrow_mut()
            .replace(stack_tray::StackTrayState::new(ZoneId(1), ZoneId(1)));
    }
    let row = {
        let app = root.app.borrow();
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        stack_tray::stack_tray_row_rect(app.viewport, anchor, 2, 1)
    };
    (root, row.x + 4.0, row.y + 4.0)
}

fn arm_test_item_drag(app: &AppState) {
    app.item_drag.borrow_mut().replace(ItemDragCandidate {
        zone_id: ZoneId(1),
        item_id: ZoneItemId(1),
        path: SmolStr::new("C:/Users/BentoDeskTest/Desktop/Child.lnk"),
        start_x: 0,
        start_y: 0,
        last_x: 0,
        last_y: 0,
        is_internal_dragging: false,
    });
}

fn assert_stack_bloom_ignores_drag(arm_drag: impl FnOnce(&AppState), expected_message: &str) {
    let (root, x, y) = stack_bloom_click_fixture();
    {
        let app = root.app.borrow();
        arm_drag(&app);
    }

    assert!(
        !handle_stack_bloom_lbutton_up(&root, std::ptr::null_mut(), x, y),
        "{expected_message}"
    );
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
    assert!(root.app.borrow().stack_tray.borrow().is_none());
}

fn assert_stack_tray_row_ignores_drag(arm_drag: impl FnOnce(&AppState), expected_message: &str) {
    let (root, x, y) = stack_tray_row_fixture();
    {
        let app = root.app.borrow();
        arm_drag(&app);
    }

    assert!(
        !handle_stack_tray_lbutton_up(&root, std::ptr::null_mut(), x, y),
        "{expected_message}"
    );
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
    assert!(root.app.borrow().stack_tray_drag.get().is_none());
}

#[test]
fn stack_bloom_petal_click_ignored_during_active_drag() {
    assert_stack_bloom_ignores_drag(
        |app| app.zone_drag.set(Some((ZoneId(2), 0, 0))),
        "zone drag owns mouse-up; stale bloom must not open tray",
    );
    assert_stack_bloom_ignores_drag(
        |app| app.zone_resize.set(Some((ZoneId(1), 180, 130))),
        "zone resize owns mouse-up; stale bloom must not open tray",
    );
    assert_stack_bloom_ignores_drag(
        arm_test_item_drag,
        "item drag owns mouse-up; stale bloom must not open tray",
    );
}

#[test]
fn stack_tray_row_click_ignored_during_active_drag() {
    assert_stack_tray_row_ignores_drag(
        |app| app.zone_drag.set(Some((ZoneId(2), 0, 0))),
        "zone drag owns mouse-up; tray row must not preview",
    );
    assert_stack_tray_row_ignores_drag(
        |app| app.zone_resize.set(Some((ZoneId(1), 180, 130))),
        "zone resize owns mouse-up; tray row must not preview",
    );
    assert_stack_tray_row_ignores_drag(
        arm_test_item_drag,
        "item drag owns mouse-up; tray row must not preview",
    );
}

#[test]
fn stack_tray_drag_still_reorders_without_normal_drag_guard() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child A", 420, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(3), "Child B", 640, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(app.zones.stack(ZoneId(1), ZoneId(3)));
        app.stack_tray
            .borrow_mut()
            .replace(stack_tray::StackTrayState::new(ZoneId(1), ZoneId(1)));
        app.stack_tray_drag
            .set(Some(stack_tray::StackTrayDragState::new(
                ZoneId(1),
                ZoneId(3),
                2,
            )));
    }
    let target_row = {
        let app = root.app.borrow();
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        stack_tray::stack_tray_row_rect(app.viewport, anchor, 3, 1)
    };

    assert!(handle_stack_tray_lbutton_up(
        &root,
        std::ptr::null_mut(),
        target_row.x + 4.0,
        target_row.y + 4.0
    ));

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::ReorderStackMember(ZoneId(1), ZoneId(3), 1))
    ));
}
