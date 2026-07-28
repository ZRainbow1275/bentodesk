#[test]
fn stack_bloom_hover_change_drives_time_based_reveal_progress() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(update_stack_bloom_hover(&app, Some(ZoneId(1)), 1_000));
        assert_eq!(app.stack_bloom_anchor.get(), Some(ZoneId(1)));
        assert!(app.stack_bloom_progress.get().abs() < f32::EPSILON);
        let reveal_duration_ms = stack_tray::stack_bloom_reveal_duration_ms(2);
        assert!(tick_stack_bloom_animation(
            &app,
            1_000 + reveal_duration_ms / 2
        ));
        let halfway = stack_bloom_reveal_progress_for_anchor(&app, ZoneId(1));
        assert!(halfway > 0.0 && halfway < 1.0);
        assert!(tick_stack_bloom_animation(&app, 1_000 + reveal_duration_ms));
        assert!((app.stack_bloom_progress.get() - 1.0).abs() < f32::EPSILON);
        assert!(!tick_stack_bloom_animation(
            &app,
            1_000 + reveal_duration_ms + 1
        ));
    }
}

#[test]
fn stack_bloom_visible_keeps_hover_frame_timer_alive_for_cursor_leave() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(update_stack_bloom_hover(&app, Some(ZoneId(1)), 1_000));
        assert!(hover_frame_pump_needed(&app));
        assert!(stack_bloom_cursor_watch_active(&app));
        assert!(hover_frame_timer_needed(&app));
        app.stack_bloom_progress.set(1.0);
        assert!(!hover_frame_pump_needed(&app));
        assert!(stack_bloom_cursor_watch_active(&app));
        assert!(hover_frame_timer_needed(&app));

        assert!(update_stack_bloom_hover(&app, None, 2_000));
        assert!(stack_bloom_cursor_watch_active(&app));
        assert!(!hover_frame_pump_needed(&app));
        assert!(hover_frame_timer_needed(&app));
        assert!(!poll_stack_bloom_interaction(
            &app,
            2_000 + stack_tray::BLOOM_LEAVE_GRACE_MS - 1
        ));
        assert!(poll_stack_bloom_interaction(
            &app,
            2_000 + stack_tray::BLOOM_LEAVE_GRACE_MS
        ));
        assert!(!stack_bloom_cursor_watch_active(&app));
        assert!(hover_frame_pump_needed(&app));

        let exit_duration_ms = stack_tray::stack_bloom_exit_duration_ms(2);
        assert!(tick_stack_bloom_animation(
            &app,
            2_000 + stack_tray::BLOOM_LEAVE_GRACE_MS + exit_duration_ms
        ));
        assert!(!hover_frame_pump_needed(&app));
        assert!(!stack_bloom_cursor_watch_active(&app));
        assert!(!hover_frame_timer_needed(&app));
    }
}

#[test]
fn stack_bloom_leave_runs_tauri_exit_window_before_clearing_anchor() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(update_stack_bloom_hover(&app, Some(ZoneId(1)), 1_000));
        assert!(tick_stack_bloom_animation(
            &app,
            1_000 + stack_tray::stack_bloom_reveal_duration_ms(2)
        ));
        assert_eq!(app.stack_bloom_anchor.get(), Some(ZoneId(1)));
        assert!(!app.stack_bloom_leaving.get());
        assert!((app.stack_bloom_progress.get() - 1.0).abs() < f32::EPSILON);

        assert!(update_stack_bloom_hover(&app, None, 2_000));
        assert_eq!(app.stack_bloom_anchor.get(), Some(ZoneId(1)));
        assert!(!app.stack_bloom_leaving.get());
        assert!((app.stack_bloom_progress.get() - 1.0).abs() < f32::EPSILON);
        assert!(!poll_stack_bloom_interaction(&app, 2_079));
        assert!(poll_stack_bloom_interaction(&app, 2_080));
        assert!(app.stack_bloom_leaving.get());
        assert!(app.stack_bloom_progress.get().abs() < f32::EPSILON);

        let exit_duration_ms = stack_tray::stack_bloom_exit_duration_ms(2);
        assert!(tick_stack_bloom_animation(
            &app,
            2_080 + exit_duration_ms / 2
        ));
        assert_eq!(app.stack_bloom_anchor.get(), Some(ZoneId(1)));
        assert!(app.stack_bloom_leaving.get());
        assert!(app.stack_bloom_progress.get() > 0.0);
        assert!(app.stack_bloom_progress.get() < 1.0);

        assert!(tick_stack_bloom_animation(&app, 2_080 + exit_duration_ms));
        assert_eq!(app.stack_bloom_anchor.get(), None);
        assert!(!app.stack_bloom_leaving.get());
        assert!((app.stack_bloom_progress.get() - 1.0).abs() < f32::EPSILON);
    }
}

#[test]
fn stack_bloom_blank_gap_reentry_cancels_leave_grace_without_blinking() {
    let root = test_app_root();
    let mut app = root.app.borrow_mut();
    app.zones
        .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
    app.zones
        .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
    assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
    assert!(update_stack_bloom_hover(&app, Some(ZoneId(1)), 1_000));
    app.stack_bloom_progress.set(1.0);

    assert!(update_stack_bloom_hover(&app, None, 2_000));
    assert!(!poll_stack_bloom_interaction(
        &app,
        2_000 + stack_tray::BLOOM_LEAVE_GRACE_MS - 1
    ));
    assert!(update_stack_bloom_hover(
        &app,
        Some(ZoneId(1)),
        2_000 + stack_tray::BLOOM_LEAVE_GRACE_MS - 1
    ));
    assert_eq!(app.stack_bloom_interaction.get().leave_started_ms, None);
    assert!(!poll_stack_bloom_interaction(&app, 2_500));
    assert_eq!(app.stack_bloom_anchor.get(), Some(ZoneId(1)));
    assert!(!app.stack_bloom_leaving.get());
    assert!((app.stack_bloom_progress.get() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn stack_bloom_visual_layer_wins_hover_over_zone_beneath_petal() {
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
        app.stack_bloom_progress.set(1.0);
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        let petal = stack_tray::stack_bloom_petal_rects(app.viewport, anchor, 2)[1];
        let x = petal.x + petal.width * 0.5;
        let y = petal.y + petal.height * 0.5;
        app.zones.add(Zone::new(
            ZoneId(3),
            "Under petal",
            (x - 80.0) as i32,
            (y - 26.0) as i32,
            180,
            130,
        ));
        (x, y)
    };
    let app = root.app.borrow();
    assert_eq!(ui::hit_test_zone(&app, x, y), Some(ZoneId(3)));
    assert_eq!(
        stack_aware_hover_zone_for_point(&app, x, y),
        Some(ZoneId(1))
    );
}

#[test]
fn stack_bloom_petal_hover_intent_then_click_commits_before_second_click_closes() {
    let (root, x, y) = stack_bloom_click_fixture();
    {
        let app = root.app.borrow();
        assert!(update_stack_bloom_petal_hover(&app, x, y, 2_000));
        assert_eq!(
            app.stack_bloom_interaction.get().active_member,
            Some(ZoneId(2))
        );
        assert!(!poll_stack_bloom_interaction(
            &app,
            2_000 + stack_tray::BLOOM_PREVIEW_HOVER_INTENT_MS - 1
        ));
        assert!(app.stack_tray.borrow().is_none());
        assert!(poll_stack_bloom_interaction(
            &app,
            2_000 + stack_tray::BLOOM_PREVIEW_HOVER_INTENT_MS
        ));
        let tray = app.stack_tray.borrow();
        let preview = tray.as_ref().expect("hover preview");
        assert!(preview.is_bloom_preview());
        assert_eq!(preview.selected_member_id, ZoneId(2));
        assert!(!app.stack_bloom_interaction.get().preview_sticky);
    }

    assert!(handle_stack_bloom_lbutton_up(
        &root,
        std::ptr::null_mut(),
        x,
        y
    ));
    consume_dispatcher(&root, std::ptr::null_mut());
    {
        let app = root.app.borrow();
        assert!(app.stack_tray.borrow().is_some());
        assert!(app.stack_bloom_interaction.get().preview_sticky);
    }

    assert!(handle_stack_bloom_lbutton_up(
        &root,
        std::ptr::null_mut(),
        x,
        y
    ));
    consume_dispatcher(&root, std::ptr::null_mut());
    let app = root.app.borrow();
    assert!(app.stack_tray.borrow().is_none());
    assert!(!app.stack_bloom_interaction.get().preview_sticky);
}

#[test]
fn pill_hover_expand_then_leave_shrinks_back_to_pill() {
    // Wave G2 — capsule pill expand-on-hover / shrink-on-leave morph
    // wiring. Models the renderer's pump: mouse enters a zone → start
    // expand; mouse leaves → start collapse; ticks drive progress.
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        // Default ZoneDisplayMode::Hover (see AppState::new) — pill is
        // collapsed unless hovered/selected.
        app.zones
            .add(Zone::new(ZoneId(1), "Compiler", 100, 100, 240, 180));
    }
    let duration = zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS;
    {
        let app = root.app.borrow();
        // 1. Pointer enters the pill.
        assert!(update_zone_pill_hover(&app, Some(ZoneId(1)), 1_000));
        assert_eq!(app.zone_pill_anim_zone.get(), Some(ZoneId(1)));
        assert!(app.zone_pill_anim_expanding.get());
        assert!(app.zone_pill_anim_progress.get().abs() < f32::EPSILON);
        assert!(app.zone_pill_anim_from_morph.get().abs() < f32::EPSILON);
        assert_eq!(app.zone_pill_anim_duration_ms.get(), duration);

        // 2. Tick halfway through the morph.
        assert!(tick_zone_pill_animation(&app, 1_000 + duration / 2));
        let halfway = app.zone_pill_anim_progress.get();
        assert!(halfway > 0.0 && halfway < 1.0);

        // 3. Tick to completion — expand finishes, slot retained so the
        //    renderer keeps the steady-state expanded body until the
        //    pointer leaves.
        assert!(tick_zone_pill_animation(&app, 1_000 + duration));
        assert!((app.zone_pill_anim_progress.get() - 1.0).abs() < f32::EPSILON);
        assert_eq!(app.zone_pill_anim_zone.get(), Some(ZoneId(1)));

        // 4. Pointer leaves — start collapse animation (same zone).
        assert!(update_zone_pill_hover(&app, None, 2_000));
        assert_eq!(app.zone_pill_anim_zone.get(), Some(ZoneId(1)));
        assert!(!app.zone_pill_anim_expanding.get());
        assert!((app.zone_pill_anim_from_morph.get() - 1.0).abs() < f32::EPSILON);

        // 5. Tick the collapse halfway.
        assert!(tick_zone_pill_animation(&app, 2_000 + duration / 2));
        let collapse_mid = app.zone_pill_anim_progress.get();
        assert!(collapse_mid > 0.0 && collapse_mid < 1.0);

        // 6. Tick to completion — pill anim slot is cleared so the
        //    renderer falls back to the steady collapsed pill chrome.
        assert!(tick_zone_pill_animation(&app, 2_000 + duration));
        assert_eq!(app.zone_pill_anim_zone.get(), None);
        assert!(!tick_zone_pill_animation(&app, 2_000 + duration + 1));
    }
}

#[test]
fn pill_hover_content_envelope_matches_fast_release_duration() {
    assert_eq!(zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS, 240);
    assert_eq!(zone_pill_geometry::ZONE_PILL_GEOMETRY_DURATION_MS, 240);
}

#[test]
fn pill_hover_reverse_keeps_the_current_visual_morph() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(1), "Compiler", 100, 100, 240, 180));
    }

    let app = root.app.borrow();
    assert!(update_zone_pill_hover(&app, Some(ZoneId(1)), 1_000));
    assert!(tick_zone_pill_animation(&app, 1_100));

    let before_leave = sampled_zone_pill_morph(&app);
    assert!(before_leave > 0.0 && before_leave < 1.0);
    assert!(update_zone_pill_hover(&app, None, 1_100));
    let after_leave = sampled_zone_pill_morph(&app);
    assert!((before_leave - after_leave).abs() < 0.0001);
    assert!(app.zone_pill_anim_duration_ms.get() < zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS);
    assert!(
        app.zone_pill_anim_duration_ms.get()
            >= zone_pill_geometry::ZONE_PILL_MIN_SEGMENT_DURATION_MS
    );

    let collapse_duration = app.zone_pill_anim_duration_ms.get();
    let reverse_at = 1_100 + collapse_duration / 4;
    assert!(tick_zone_pill_animation(&app, reverse_at));
    let before_reenter = sampled_zone_pill_morph(&app);
    assert!(before_reenter > 0.0 && before_reenter < before_leave);
    assert!(update_zone_pill_hover(&app, Some(ZoneId(1)), reverse_at));
    let after_reenter = sampled_zone_pill_morph(&app);
    assert!((before_reenter - after_reenter).abs() < 0.0001);
}

#[test]
fn header_close_animates_a_click_selected_panel_from_its_visible_shape() {
    let mut app = AppState::new();
    app.zones
        .add(Zone::new(ZoneId(1), "Compiler", 100, 120, 240, 180));
    app.set_zone_display_mode(ZoneDisplayMode::Click);
    app.selected_zone.set(Some(ZoneId(1)));

    assert!(app.zone_pill_body_visible(app.zones.get(ZoneId(1)).unwrap()));
    assert_eq!(app.zone_pill_anim_zone.get(), None);
    assert!(collapse_zone_from_header(&app, ZoneId(1), 1_500));
    assert_eq!(app.selected_zone.get(), None);
    assert_eq!(app.zone_pill_anim_zone.get(), Some(ZoneId(1)));
    assert!(!app.zone_pill_anim_expanding.get());
    assert!((app.zone_pill_anim_from_morph.get() - 1.0).abs() < f32::EPSILON);
    assert!(app.zone_pill_anim_progress.get().abs() < f32::EPSILON);
    assert!(app.zone_pill_morph_in_flight(app.zones.get(ZoneId(1)).unwrap()));
    assert!((sampled_zone_pill_morph(&app) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn live_drag_geometry_updates_memory_before_dispatcher_drain() {
    let mut app = AppState::new();
    app.zones
        .add(Zone::new(ZoneId(1), "Compiler", 100, 120, 240, 180));

    assert!(move_zone_live(
        &mut app,
        ZoneId(1),
        DispatchPoint::new(220, 260)
    ));
    let moved = app.zones.get(ZoneId(1)).expect("moved zone");
    assert_eq!((moved.x, moved.y), (220, 260));
    assert!(
        app.dirty.get(),
        "live drag update must defer one release-time write"
    );

    app.dirty.set(false);
    assert!(resize_zone_live(
        &mut app,
        ZoneId(1),
        DispatchSize::new(300, 40)
    ));
    let resized = app.zones.get(ZoneId(1)).expect("resized zone");
    assert_eq!((resized.w, resized.h), (300, 60));
    assert!(app.dirty.get());
}

#[test]
fn live_drag_moves_an_existing_stack_as_one_rigid_cluster() {
    let mut app = AppState::new();
    app.zones
        .add(Zone::new(ZoneId(1), "Anchor", 100, 120, 240, 180));
    app.zones
        .add(Zone::new(ZoneId(2), "Child A", 140, 190, 240, 180));
    app.zones
        .add(Zone::new(ZoneId(3), "Child B", 60, 250, 240, 180));
    assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
    assert!(app.zones.stack(ZoneId(1), ZoneId(3)));

    assert!(move_zone_live(
        &mut app,
        ZoneId(1),
        DispatchPoint::new(300, 320)
    ));
    assert_eq!(
        app.zones.get(ZoneId(1)).map(|zone| (zone.x, zone.y)),
        Some((300, 320))
    );
    assert_eq!(
        app.zones.get(ZoneId(2)).map(|zone| (zone.x, zone.y)),
        Some((340, 390))
    );
    assert_eq!(
        app.zones.get(ZoneId(3)).map(|zone| (zone.x, zone.y)),
        Some((260, 450))
    );
    assert!(app.dirty.get());
}

#[test]
fn zone_drag_pointer_offset_centers_the_painted_capsule_not_the_panel() {
    let mut app = AppState::new();
    app.zones
        .add(Zone::new(ZoneId(1), "Expanded", 100, 120, 800, 600));
    app.zones
        .add(Zone::new(ZoneId(2), "Stack child", 400, 120, 800, 600));

    assert_eq!(zone_drag_pointer_offset(&app, ZoneId(1)), Some((80, 24)));
    assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
    assert_eq!(zone_drag_pointer_offset(&app, ZoneId(1)), Some((110, 26)));
}

#[test]
fn header_close_clears_mouse_down_selection_and_scheduler_expansion() {
    let mut app = AppState::new();
    let zone = Zone::new(ZoneId(1), "Compiler", 100, 120, 240, 180);
    app.zones.add(zone);
    app.set_zone_display_mode(ZoneDisplayMode::Hover);
    app.selected_zone.set(Some(ZoneId(1)));
    app.hovered_zone.set(Some(ZoneId(1)));
    app.hover_scheduler.set({
        let mut scheduler = zone_pill_geometry::HoverScheduler::new();
        scheduler.mark_expanded(ZoneId(1), 1_000);
        scheduler
    });
    app.zone_pill_anim_zone.set(Some(ZoneId(1)));
    app.zone_pill_anim_expanding.set(true);
    app.zone_pill_anim_progress.set(1.0);

    assert!(app.zone_pill_body_visible(app.zones.get(ZoneId(1)).unwrap()));
    assert!(collapse_zone_from_header(&app, ZoneId(1), 1_500));
    assert_eq!(app.selected_zone.get(), None);
    assert_eq!(app.hover_scheduler.get().expanded_zone(), None);
    assert_eq!(app.hovered_zone.get(), Some(ZoneId(1)));
    assert!(!app.zone_pill_anim_expanding.get());
    assert!(!app.zone_pill_body_visible(app.zones.get(ZoneId(1)).unwrap()));
}

#[test]
fn pointer_drag_reset_clears_hover_morph_channels_without_tray_drag_guard() {
    let mut app = AppState::new();
    app.zones
        .add(Zone::new(ZoneId(1), "Compiler", 100, 120, 240, 180));
    app.hovered_zone.set(Some(ZoneId(1)));
    app.stack_bloom_anchor.set(Some(ZoneId(1)));
    app.stack_bloom_leaving.set(true);
    app.stack_bloom_progress.set(0.25);
    app.zone_pill_anim_zone.set(Some(ZoneId(1)));
    app.zone_pill_anim_expanding.set(true);
    app.zone_pill_anim_progress.set(0.5);
    app.zone_pill_anim_started_ms.set(900);
    app.hover_scheduler.set({
        let mut scheduler = zone_pill_geometry::HoverScheduler::new();
        scheduler.on_enter(ZoneId(1), 1_000, 150);
        scheduler
    });
    app.item_hover
        .set(bento_nano_app::business::item_card::ItemHoverState::new());
    {
        let mut item_hover = app.item_hover.get();
        assert!(item_hover.on_hover(Some((ZoneId(1), ZoneItemId(1))), 1_000));
        app.item_hover.set(item_hover);
    }
    app.highlight_overlay.borrow_mut().set_targets([
        bento_nano_app::business::highlight_overlay::HighlightRect::new(112.0, 144.0, 80.0, 56.0),
    ]);
    {
        let mut anim = app.pill_animator.borrow_mut();
        anim.start(
            ZoneId(1),
            bento_nano_app::animator::AnimChannel::PillHover,
            1_000,
            bento_nano_app::animator::HOVER_IN_DURATION_MS,
            0.0,
            1.0,
            bento_nano_app::animator::Easing::EaseOutCubic,
        );
        anim.start(
            ZoneId(1),
            bento_nano_app::animator::AnimChannel::PillPress,
            1_000,
            bento_nano_app::animator::PRESS_DOWN_DURATION_MS,
            0.0,
            1.0,
            bento_nano_app::animator::Easing::EaseOutCubic,
        );
    }
    app.zone_drag.set(Some((ZoneId(1), 4, 4)));

    assert!(normal_pointer_drag_active(&app));
    reset_pointer_drag_hover_channels(&app, Some(ZoneId(1)), 1_020);

    assert_eq!(app.hovered_zone.get(), None);
    assert_eq!(app.stack_bloom_anchor.get(), None);
    assert!(!app.stack_bloom_leaving.get());
    assert_eq!(app.stack_bloom_progress.get(), 1.0);
    assert_eq!(app.zone_pill_anim_zone.get(), None);
    assert_eq!(app.zone_pill_anim_progress.get(), 1.0);
    assert_eq!(app.zone_pill_anim_from_morph.get(), 0.0);
    assert_eq!(
        app.zone_pill_anim_duration_ms.get(),
        zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS
    );
    assert!(!app.hover_scheduler.get().is_pending());
    assert!(!app.item_hover.get().is_active(1_020));
    assert!(
        !app.highlight_overlay.borrow().has_targets(),
        "normal pointer drag must clear Search/Suggestor highlight chrome"
    );
    let anim = app.pill_animator.borrow();
    assert_eq!(
        anim.sample(
            ZoneId(1),
            bento_nano_app::animator::AnimChannel::PillHover,
            1_020
        ),
        0.0
    );
    assert_eq!(
        anim.sample(
            ZoneId(1),
            bento_nano_app::animator::AnimChannel::PillPress,
            1_020
        ),
        0.0
    );
    drop(anim);

    app.zone_drag.set(None);
    app.stack_tray_drag
        .set(Some(stack_tray::StackTrayDragState::new(
            ZoneId(1),
            ZoneId(1),
            0,
        )));
    assert!(
        !normal_pointer_drag_active(&app),
        "StackTray row reorder must not be blocked by the normal drag guard"
    );
}

#[test]
fn pill_animator_tick_does_not_repaint_for_item_only_status_dot_pulse() {
    let mut app = AppState::new();
    let mut zone = Zone::new(ZoneId(1), "Compiler", 100, 120, 240, 180);
    zone.items.push(ZoneItem::new(
        ZoneItemId(1),
        r"C:\Users\Alice\Desktop\main.rs",
        "rust-icon",
        0,
        0,
    ));
    app.zones.add(zone);

    assert!(
        !tick_pill_animator(&app, 10_000),
        "item presence alone must not keep the main window repainting"
    );

    {
        let mut anim = app.pill_animator.borrow_mut();
        anim.start(
            ZoneId(1),
            bento_nano_app::animator::AnimChannel::PillHover,
            10_000,
            bento_nano_app::animator::HOVER_IN_DURATION_MS,
            0.0,
            1.0,
            bento_nano_app::animator::Easing::EaseOutCubic,
        );
    }

    assert!(
        tick_pill_animator(&app, 10_040),
        "real hover/press animation entries must still keep repaint cadence alive"
    );
}

#[test]
fn stack_zone_does_not_auto_open_tray_but_open_stack_tray_does() {
    // #4 (2026-06-02) — the 3-state stack machine: creating a stack must
    // NOT pop the management tray (it leaves the anchor as the compact
    // pill). Only the explicit Command::OpenStackTray opens it, and
    // CloseStackTray closes it again. Pure-CPU state assertion.
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
    }

    // Create the stack via the dispatcher — the tray must stay closed.
    root.dispatcher
        .push(Command::StackZone(ZoneId(1), ZoneId(2)));
    consume_dispatcher(&root, std::ptr::null_mut());
    {
        let app = root.app.borrow();
        assert_eq!(app.zones.stack_anchor_for(ZoneId(2)), Some(ZoneId(1)));
        assert_eq!(
            app.pill_animator.borrow().occupancy(),
            1,
            "StackZone must start exactly one bounded emerge animation"
        );
        assert!(
            app.stack_tray.borrow().is_none(),
            "StackZone must NOT auto-open the management tray"
        );
        app.hovered_zone.set(Some(ZoneId(1)));
        app.stack_bloom_anchor.set(Some(ZoneId(1)));
        app.stack_bloom_leaving.set(true);
        app.stack_bloom_progress.set(0.5);
        drive_hover_scheduler(&app, Some(ZoneId(2)), 1_000);
        assert!(
            app.hover_scheduler.get().is_pending(),
            "fixture should start with pending hover state so OpenStackTray proves cleanup"
        );
    }

    // Explicit open — tray becomes Some, anchored on the stack.
    root.dispatcher.push(Command::OpenStackTray(ZoneId(1)));
    consume_dispatcher(&root, std::ptr::null_mut());
    {
        let app = root.app.borrow();
        assert!(
            app.stack_tray.borrow().is_some(),
            "OpenStackTray is the sole opener and must set stack_tray"
        );
        assert_eq!(
            app.hovered_zone.get(),
            None,
            "OpenStackTray must clear stale hover target"
        );
        assert_eq!(
            app.stack_bloom_anchor.get(),
            None,
            "OpenStackTray must clear the mutually-exclusive hover bloom"
        );
        assert!(!app.stack_bloom_leaving.get());
        assert_eq!(app.stack_bloom_progress.get(), 1.0);
        assert!(
            !app.hover_scheduler.get().is_pending(),
            "OpenStackTray must cancel pending hover scheduler work"
        );
    }

    // Explicit close — back to None.
    root.dispatcher.push(Command::CloseStackTray);
    consume_dispatcher(&root, std::ptr::null_mut());
    assert!(root.app.borrow().stack_tray.borrow().is_none());
}
