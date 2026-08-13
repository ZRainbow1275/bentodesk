#[test]
fn stack_bloom_preview_item_hit_tracks_inline_search_reveal_frames() {
    const START_MS: u32 = 10_000;
    const DURATION_MS: u32 = 1_000;
    let root = open_stack_bloom_preview();
    {
        let mut app = root.app.borrow_mut();
        let member = app.zones.get_mut(ZoneId(2)).expect("member");
        member.w = 320;
        member.h = 360;
        member.items.push(ZoneItem::new(
            ZoneItemId(42),
            r"C:\Users\Alice\Desktop\animated.txt",
            "text-icon",
            0,
            0,
        ));
        app.zone_search_target.set(Some(ZoneId(2)));
        app.zone_search_closing.set(false);
        app.pill_animator.borrow_mut().start(
            ZoneId(2),
            bentodesk_app::animator::AnimChannel::InlineSearch,
            START_MS,
            DURATION_MS,
            0.0,
            1.0,
            bentodesk_app::animator::Easing::Linear,
        );
    }
    let preview = stack_bloom_preview_rect_for_test(&root);

    for (elapsed, expected_progress) in [
        (0, 0.0),
        (250, 0.25),
        (500, 0.5),
        (750, 0.75),
        (1_000, 1.0),
    ] {
        let app = root.app.borrow();
        let now_ms = START_MS + elapsed;
        app.geometry_frame_now_ms.set(now_ms);
        let progress = app.zone_search_animation_progress_at(now_ms);
        assert!((progress - expected_progress).abs() < 0.0001);
        let member = app.zones.get(ZoneId(2)).expect("member");
        let offset =
            bentodesk_app::business::search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX * progress;
        let card = highlight_overlay::item_card_rect_for_flow_slot_in_panel(
            member, preview, 0, false, offset,
        )
        .0;

        assert_eq!(
            stack_bloom_preview_item_hit_for_point(
                &app,
                card.x + card.width * 0.5,
                card.y + card.height * 0.5,
            ),
            Some((ZoneId(1), ZoneId(2), ZoneItemId(42))),
            "focused preview hit drifted from the painted card at progress {expected_progress}",
        );
    }
}

#[test]
fn item_drag_reset_keeps_hover_panel_and_bloom_preview_as_internal_targets() {
    let root = test_app_root();
    let (hover_x, hover_y) = {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1_280.0,
            height: 720.0,
        };
        let mut zone = Zone::new(ZoneId(50), "Hover", 100, 100, 320, 360);
        zone.items.push(ZoneItem::new(
            ZoneItemId(51),
            r"C:\Users\Alice\Desktop\hover.txt",
            "text-icon",
            0,
            0,
        ));
        app.zones.add(zone);
        app.set_zone_display_mode(ZoneDisplayMode::Hover);
        app.hovered_zone.set(Some(ZoneId(50)));
        let mut scheduler = app.hover_scheduler.get();
        scheduler.mark_expanded(ZoneId(50), 1_000);
        app.hover_scheduler.set(scheduler);
        let zone = app.zones.get(ZoneId(50)).expect("hover zone");
        let panel = app.zone_expanded_placement(zone).panel;
        let card = highlight_overlay::item_card_rect_for_flow_slot_in_panel(
            zone, panel, 0, false, 0.0,
        )
        .0;
        let x = card.x + card.width * 0.5;
        let y = card.y + card.height * 0.5;
        app.item_drag.borrow_mut().replace(ItemDragCandidate {
            zone_id: ZoneId(50),
            item_id: ZoneItemId(51),
            path: SmolStr::new(r"C:\Users\Alice\Desktop\hover.txt"),
            start_x: x as i32,
            start_y: y as i32,
            last_x: x as i32,
            last_y: y as i32,
            is_internal_dragging: true,
        });
        (x, y)
    };
    {
        let app = root.app.borrow();
        assert!(!should_start_item_drag_out(&app, hover_x, hover_y, false));
        reset_item_drag_hover_channels(&app, 1_010);
        assert_eq!(app.hover_scheduler.get().expanded_zone(), Some(ZoneId(50)));
        assert!(app.zone_pill_body_visible(app.zones.get(ZoneId(50)).expect("hover zone")));
        assert!(!should_start_item_drag_out(&app, hover_x, hover_y, false));
    }
    clear_hover(&root);
    {
        let app = root.app.borrow();
        assert_eq!(app.hover_scheduler.get().expanded_zone(), Some(ZoneId(50)));
        assert!(!should_start_item_drag_out(&app, hover_x, hover_y, false));
    }

    let root = open_stack_bloom_preview();
    let preview = stack_bloom_preview_rect_for_test(&root);
    let (preview_x, preview_y) = {
        let mut app = root.app.borrow_mut();
        let member = app.zones.get_mut(ZoneId(2)).expect("member");
        member.w = 320;
        member.h = 360;
        member.items.push(ZoneItem::new(
            ZoneItemId(52),
            r"C:\Users\Alice\Desktop\preview.txt",
            "text-icon",
            0,
            0,
        ));
        let card = highlight_overlay::item_card_rect_for_flow_slot_in_panel(
            member, preview, 0, false, 0.0,
        )
        .0;
        let x = card.x + card.width * 0.5;
        let y = card.y + card.height * 0.5;
        app.item_drag.borrow_mut().replace(ItemDragCandidate {
            zone_id: ZoneId(2),
            item_id: ZoneItemId(52),
            path: SmolStr::new(r"C:\Users\Alice\Desktop\preview.txt"),
            start_x: x as i32,
            start_y: y as i32,
            last_x: x as i32,
            last_y: y as i32,
            is_internal_dragging: true,
        });
        (x, y)
    };
    {
        let app = root.app.borrow();
        assert!(!should_start_item_drag_out(&app, preview_x, preview_y, false));
        reset_item_drag_hover_channels(&app, 2_010);
        assert!(
            app.stack_tray
                .borrow()
                .as_ref()
                .is_some_and(|state| state.is_bloom_preview())
        );
        assert!(!should_start_item_drag_out(&app, preview_x, preview_y, false));
    }
    clear_hover(&root);
    {
        let app = root.app.borrow();
        assert!(
            app.stack_tray
                .borrow()
                .as_ref()
                .is_some_and(|state| state.is_bloom_preview())
        );
        assert!(!should_start_item_drag_out(&app, preview_x, preview_y, false));
    }
}
